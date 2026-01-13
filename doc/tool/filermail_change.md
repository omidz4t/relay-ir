# Plan: Rust Implementation of `filtermail.py`

This plan outlines the steps to replace the Python-based `filtermail.py` with a Rust implementation, ensuring it remains a drop-in replacement for the current system.

## Proposed Changes

### [Component Name] filtermail-rust (New Component)

We will create a new Rust crate (e.g., in `filtermail-rust/`) that implements the SMTP filtering logic currently in `filtermail.py`.

#### [NEW] [filtermail-rust](file:///home/piker/Projects/other/deltachat/relay/filtermail-rust)
- Implementation of the SMTP server using a Rust library like `mail-auth` or `toxic-smtp` (or just `async-trait` + `tokio` for a simple SMTP server).
- Port of the OpenPGP packet parsing logic (`check_openpgp_payload`).
- Port of the MIME analysis logic.
- Integration with the existing `chatmail.ini` configuration.

## Verification Plan

To ensure the Rust implementation is a full drop-in replacement, we will follow a multi-stage verification process.

### Automated Tests

#### 1. Reuse Existing Python Tests
The most powerful verification is to run the existing `pytest` suite against the Rust binary.
- **Command**: `pytest chatmaild/src/chatmaild/tests/test_filtermail_blackbox.py`
- **Setup**: We will modify the test environment (or the `filtermail` entry point) to point to the Rust binary instead of the Python script.

#### 2. Parity Testing Script
Create a "Differential Testing" script that feeds the same set of email samples (good and bad) to both the Python and Rust versions and compares the SMTP response codes and error messages.
- **Samples**: Use the existing `.eml` files in `chatmaild/src/chatmaild/tests/data/`.

#### 3. Rust Unit Tests
Port the unit tests from `test_filtermail.py` to Rust `#[test]` functions:
- OpenPGP packet parsing logic.
- Armored payload validation.
- Rate limiter logic.
- Secure-join detection.

### Manual Verification
1.  **Local Deployment**: Install the Rust binary as the `filtermail` service on a test mail server.
2.  **End-to-End Mail Flow**: Send encrypted and unencrypted mail (using Delta Chat or `swaks`) and verify that:
    *   Encrypted mail is accepted and delivered.
    *   Unencrypted mail is rejected with the exact same error message: `523 Encryption Needed: Invalid Unencrypted Mail`.
    *   Rate limiting works as expected.

### Performance & Memory
- Compare memory usage and throughput between Python and Rust versions under load (optional but recommended).

## User Review Required

> [!IMPORTANT]
> The Rust version must handle `chatmail.ini` identically. We need to decide whether to use a Rust INI parser that matches the Python `configparser` behavior or call a small helper to get variables.
