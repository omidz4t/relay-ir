use base64::{engine::general_purpose, Engine as _};
use mail_parser::{Message, PartType, MimeHeaders};

pub const ENCRYPTION_NEEDED_523: &str = "523 Encryption Needed: Invalid Unencrypted Mail";

pub fn check_openpgp_payload(payload: &[u8]) -> bool {
    let mut i = 0;
    while i < payload.len() {
        // Only OpenPGP format is allowed.
        if payload[i] & 0xC0 != 0xC0 {
            return false;
        }

        let packet_type_id = payload[i] & 0x3F;
        i += 1;

        if i >= payload.len() {
            eprintln!("REJECT: OpenPGP payload: Unexpected end of payload after packet tag");
            return false;
        }

        while payload[i] >= 224 && payload[i] < 255 {
            // Partial body length.
            let partial_length = 1 << (payload[i] & 0x1F);
            i += 1 + partial_length;
            if i >= payload.len() {
                eprintln!("REJECT: OpenPGP payload: Unexpected end of payload during partial body length processing");
                return false;
            }
        }

        let body_len: usize;
        if payload[i] < 192 {
            // One-octet length.
            body_len = payload[i] as usize;
            i += 1;
        } else if payload[i] < 224 {
            // Two-octet length.
            if i + 1 >= payload.len() {
                eprintln!("REJECT: OpenPGP payload: Unexpected end of payload during two-octet length processing");
                return false;
            }
            body_len = (((payload[i] as usize) - 192) << 8) + (payload[i + 1] as usize) + 192;
            i += 2;
        } else if payload[i] == 255 {
            // Five-octet length.
            if i + 4 >= payload.len() {
                return false;
            }
            body_len = ((payload[i + 1] as usize) << 24)
                | ((payload[i + 2] as usize) << 16)
                | ((payload[i + 3] as usize) << 8)
                | (payload[i + 4] as usize);
            i += 5;
        } else {
            // Impossible, partial body length was processed above.
            eprintln!("REJECT: OpenPGP payload: Invalid length octet value");
            return false;
        }

        i += body_len;

        if i == payload.len() {
            // Last packet should be
            // Symmetrically Encrypted and Integrity Protected Data Packet (SEIPD)
            return packet_type_id == 18;
        } else if i > payload.len() {
            return false;
        } else if packet_type_id != 1 && packet_type_id != 3 {
            // All packets except the last one must be either
            // Public-Key Encrypted Session Key Packet (PKESK)
            // or
            // Symmetric-Key Encrypted Session Key Packet (SKESK)
            return false;
        }
    }

    false
}

pub fn check_armored_payload(payload: &str, outgoing: bool) -> bool {
    let prefix = "-----BEGIN PGP MESSAGE-----";
    if !payload.contains(prefix) {
        eprintln!("REJECT: Missing BEGIN PGP MESSAGE prefix");
        return false;
    }
    let start_idx = payload.find(prefix).unwrap();
    let mut payload = &payload[start_idx + prefix.len()..];
    
    // Skip any header whitespace
    payload = payload.trim_start();

    let suffix = "-----END PGP MESSAGE-----";
    if !payload.contains(suffix) {
        eprintln!("REJECT: Missing END PGP MESSAGE suffix");
        return false;
    }
    let end_idx = payload.find(suffix).unwrap();
    payload = &payload[..end_idx];

    let version_comment = "Version: ";
    if payload.starts_with(version_comment) {
        if outgoing {
            // Disallow comments in outgoing messages
            eprintln!("REJECT: Outgoing armored payload contains 'Version:' comment");
            return false;
        }
        // Remove comments from incoming messages
        if let Some((_, rest)) = payload.split_once("\r\n") {
            payload = rest;
        } else {
            eprintln!("REJECT: Armored payload 'Version:' line without CRLF");
            return false;
        }
    }

    while payload.starts_with("\r\n") {
        payload = &payload[2..];
    }

    // Remove CRC24.
    if let Some((base64_part, _)) = payload.rsplit_once('=') {
        payload = base64_part;
    }

    // Some implementations might have whitespace/newlines in base64
    let cleaned_payload: String = payload.chars().filter(|c| !c.is_whitespace()).collect();

    match general_purpose::STANDARD.decode(cleaned_payload) {
        Ok(decoded) => check_openpgp_payload(&decoded),
        Err(_) => false,
    }
}

pub fn is_securejoin(message: &Message) -> bool {
    let sj_header = message.header("secure-join");
    let sj_val = sj_header.and_then(|h| h.as_text());
    
    if !matches!(sj_val, Some("vc-request") | Some("vg-request")) {
        return false;
    }

    let mut parts_count = 0;
    for part in message.parts.iter().filter(|p| {
        p.content_type().map_or(true, |ct| ct.c_type.to_lowercase() != "multipart")
    }) {
        parts_count += 1;
        if parts_count > 1 {
            eprintln!("REJECT: securejoin has more than 1 non-multipart part");
            return false;
        }

        if !part.is_content_type("text", "plain") {
            eprintln!("REJECT: securejoin part is not text/plain: {:?}", part.content_type());
            return false;
        }

        if let PartType::Text(text) = &part.body {
            let payload = text.trim().to_lowercase();
            if payload != "secure-join: vc-request" && payload != "secure-join: vg-request" {
                eprintln!("REJECT: securejoin invalid payload: {}", payload);
                return false;
            }
        }
    }
    parts_count == 1
}

pub fn check_encrypted(message: &Message, outgoing: bool) -> bool {
    if !message.is_content_type("multipart", "encrypted") {
        return false;
    }

    let mut parts = message.parts.iter().filter(|p| {
        p.content_type().map_or(true, |ct| ct.c_type.to_lowercase() != "multipart")
    });
    
    // Part 0: application/pgp-encrypted
    let part0 = match parts.next() {
        Some(p) => p,
        None => {
            eprintln!("REJECT: Missing part 0 in encrypted mail");
            return false;
        },
    };
    if !part0.is_content_type("application", "pgp-encrypted") {
        eprintln!("REJECT: Part 0 is not application/pgp-encrypted: {:?}", part0.content_type());
        return false;
    }
    if let PartType::Text(text) = &part0.body {
        if text.trim() != "Version: 1" {
            return false;
        }
    }

    // Part 1: application/octet-stream
    let part1 = match parts.next() {
        Some(p) => p,
        None => {
            eprintln!("REJECT: Missing part 1 in encrypted mail");
            return false;
        },
    };
    if !part1.is_content_type("application", "octet-stream") {
        eprintln!("REJECT: Part 1 is not application/octet-stream: {:?}", part1.content_type());
        return false;
    }
    
    if let PartType::Text(text) = &part1.body {
        if !check_armored_payload(text, outgoing) {
            return false;
        }
    } else if let PartType::Binary(bin) = &part1.body {
        if let Ok(text) = std::str::from_utf8(bin) {
            if !check_armored_payload(text, outgoing) {
                return false;
            }
        } else {
            eprintln!("REJECT: Part 1 is binary and not valid UTF-8");
            return false;
        }
    } else {
        eprintln!("REJECT: Part 1 is not text or binary");
        return false;
    }

    // Should have exactly 2 parts
    if let Some(extra) = parts.next() {
        eprintln!("REJECT: Encrypted mail has more than 2 parts. Extra part type: {:?}", extra.content_type());
        return false;
    }

    true
}
