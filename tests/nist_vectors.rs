//! SHA-256 verification against NIST FIPS 180-4 test vectors.
//!
//! ADR-0004 section 5 condition 5 requires that hashing correctness be
//! verified rather than assumed. The project cannot audit the dependency's
//! unsafe code, but it can establish that the implementation produces the
//! published digests for published inputs.
//!
//! Vectors are from the NIST Cryptographic Algorithm Validation Program
//! SHA-256 examples.

use taphonomy::hash::hash_reader;

fn digest_hex(input: &[u8]) -> String {
    let mut reader = input;
    hash_reader(&mut reader)
        .expect("hashing an in-memory slice cannot fail")
        .digest
        .to_hex()
}

#[test]
fn nist_empty_string() {
    assert_eq!(
        digest_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn nist_abc() {
    assert_eq!(
        digest_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn nist_two_block_message() {
    let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    assert_eq!(
        digest_hex(input),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn nist_one_million_a() {
    let input = vec![b'a'; 1_000_000];
    assert_eq!(
        digest_hex(&input),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

/// Input larger than one read buffer must produce the same digest as the
/// equivalent single-shot hash. Guards against a chunking defect in
/// `hash_reader` rather than in the dependency.
#[test]
fn chunk_boundary_does_not_affect_digest() {
    let input = vec![b'x'; 64 * 1024 * 3 + 1];

    let mut whole = input.as_slice();
    let a = hash_reader(&mut whole).expect("hashing slice").digest;

    struct DripFeed<'a> {
        data: &'a [u8],
        pos: usize,
    }

    impl std::io::Read for DripFeed<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let n = buf.len().min(7).min(self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    let mut drip = DripFeed {
        data: &input,
        pos: 0,
    };
    let b = hash_reader(&mut drip).expect("hashing drip feed").digest;

    assert_eq!(a, b, "digest must not depend on read chunk size");
}
