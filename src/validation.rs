//! Validation of recovered data against a reference.
//!
//! ADR-0013. M7 ends with a digest of the bytes a deleted entry's implied
//! run holds. This module decides what that digest means when the operator
//! supplies a digest of the file they are looking for, and says so when
//! they supply none.
//!
//! Comparing two digests is not a filesystem operation, so this module
//! reads no evidence and names no filesystem structure. ADR-0013 section
//! 11; `docs/ARCHITECTURE.md` section 5.6 requires validation to remain
//! separate from extraction.
//!
//! Nothing here is secret. The reference is supplied by the operator and
//! reported back to them, so comparison is by value and constant-time
//! comparison is neither required nor wanted: ADR-0013 section 17.

use crate::hash::Sha256Digest;

/// What comparing a recovered digest against a reference established.
///
/// None of these is a confidence level. ADR-0003 section 8 makes that
/// taxonomy binding on the first code that assigns one, which under
/// ADR-0002 section 8 is M9. These are the evidence M9 assigns from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The recovered bytes are byte-for-byte the reference's.
    ///
    /// This establishes byte equality between the two hashed objects and
    /// nothing beyond it. `docs/SAFETY.md` section 10.
    Match,
    /// The recovered bytes are not the reference's.
    ///
    /// At least four conditions produce this and the evidence does not
    /// distinguish between them: the file was fragmented, clusters of the
    /// run were reused without the FAT recording it, the entry's size
    /// field does not describe the content, or the reference is a
    /// different file. ADR-0013 section 9 forbids choosing among them.
    Differs,
    /// No reference was supplied, so nothing was compared.
    ///
    /// The absence of a comparison, not a passed one. ADR-0003 section 4.4
    /// requires an absence of evidence to fail closed rather than read as
    /// a result.
    NotAttempted,
}

/// A comparison, with the extent it covered.
///
/// The digest and the number of bytes it covers travel together, as they do
/// in [`crate::fat_recovery::Extraction`], so that an outcome cannot be
/// read apart from what it applies to. ADR-0013 section 10.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Validation {
    /// What the comparison established.
    pub outcome: Outcome,
    /// Digest of the bytes that were recovered.
    pub recovered: Sha256Digest,
    /// Digest the operator supplied, where they supplied one.
    pub reference: Option<Sha256Digest>,
    /// Number of bytes `recovered` covers.
    pub covers: u64,
}

/// Compares a recovered digest against the operator's reference.
///
/// `covers` is the number of bytes `recovered` was computed over, which the
/// caller takes from the same extraction as the digest itself. This
/// function reads no evidence and so cannot check that pairing; supplying a
/// digest with a byte count from elsewhere would misstate the extent of a
/// finding.
///
/// A `reference` of `None` yields [`Outcome::NotAttempted`], which is not an
/// error. Nothing failed: the operator asked for no comparison.
pub fn validate(
    recovered: Sha256Digest,
    covers: u64,
    reference: Option<Sha256Digest>,
) -> Validation {
    let outcome = match reference {
        Some(supplied) if supplied == recovered => Outcome::Match,
        Some(_) => Outcome::Differs,
        None => Outcome::NotAttempted,
    };

    Validation {
        outcome,
        recovered,
        reference,
        covers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::DIGEST_LEN;

    fn digest(byte: u8) -> Sha256Digest {
        Sha256Digest::from_bytes([byte; DIGEST_LEN])
    }

    #[test]
    fn equal_digests_match() {
        let validation = validate(digest(0xab), 1600, Some(digest(0xab)));

        assert_eq!(validation.outcome, Outcome::Match);
        assert_eq!(validation.recovered, digest(0xab));
        assert_eq!(validation.reference, Some(digest(0xab)));
        assert_eq!(validation.covers, 1600);
    }

    /// One byte is enough. A digest is compared whole or not at all.
    #[test]
    fn digests_differing_in_one_byte_differ() {
        let mut bytes = [0xabu8; DIGEST_LEN];
        bytes[DIGEST_LEN - 1] = 0xaa;
        let reference = Sha256Digest::from_bytes(bytes);

        let validation = validate(digest(0xab), 1600, Some(reference));

        assert_eq!(validation.outcome, Outcome::Differs);
        assert_eq!(validation.reference, Some(reference));
    }

    /// No reference is neither a pass nor a failure.
    #[test]
    fn no_reference_is_not_attempted() {
        let validation = validate(digest(0xab), 1600, None);

        assert_eq!(validation.outcome, Outcome::NotAttempted);
        assert_eq!(validation.reference, None);
    }

    /// The extent is carried whatever the outcome, so no finding can be
    /// reported without it.
    #[test]
    fn every_outcome_carries_the_extent() {
        for reference in [Some(digest(0xab)), Some(digest(0x00)), None] {
            let validation = validate(digest(0xab), 4096, reference);

            assert_eq!(validation.covers, 4096);
            assert_eq!(validation.recovered, digest(0xab));
        }
    }
}
