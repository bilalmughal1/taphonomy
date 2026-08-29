# ADR-0005: Initial Partition Scheme Target

* **Status:** Accepted
* **Date:** 2026-08-29
* **Decision owners:** Taphonomy project
* **Scope:** Partition table scheme for milestone M2
* **Related:** ADR-0002 §8 (M2); `DEVELOPMENT_ENVIRONMENT.md` §18

---

## 1. Context

ADR-0002 §8 defines milestone M2 as "Detect the partition table and locate
partitions" without naming a scheme.

Two schemes are in general use:

**MBR** (Master Boot Record). A 512-byte structure at LBA 0 containing a
signature at offset `0x1FE` and four 16-byte partition entries beginning at
offset `0x1BE`. Addresses up to 2 TB with 512-byte sectors.

**GPT** (GUID Partition Table). A protective MBR at LBA 0, a header at LBA 1,
and a partition entry array normally beginning at LBA 2. Carries CRC32
checksums over the header and entry array, and a backup copy at the end of
the device.

Supporting both in M2 doubles its scope and introduces a CRC32
implementation decision. Supporting one means the evidence abstraction is
designed against a single scheme and may need revision when the second is
added.

---

## 2. Decision

**MBR is the initial partition scheme target.**

GPT is deferred, not rejected. It is required before Taphonomy can process
SDUC media or modern internal drives.

---

## 3. Evidence

### 3.1 Removable media below 2 TB uses MBR

The SD Association states that SDXC is formatted with the Master Boot Record
format, which is limited to 2 TB, and that SDUC is formatted with GUID
Partition Table format in order to exceed that limit.

Source: SD Association, "Growing Demand, Growing Capacity: An introduction to
the new SD Ultra Capacity function".
<https://www.sdcard.org/press/thoughtleadership/growing-demand-growing-capacity-an-introduction-to-the-new-sd-ultra-capacity-function/>

The SD capacity standards are:

| Standard | Capacity | Partition scheme |
|---|---|---|
| SDSC | up to 2 GB | MBR |
| SDHC | up to 32 GB | MBR |
| SDXC | over 32 GB to 2 TB | MBR |
| SDUC | over 2 TB to 128 TB | GPT |

The boundary is capacity, not recency. Every SD card at or below 2 TB is MBR
by specification.

### 3.2 SDUC media is not yet in general circulation

SDUC was announced in 2018 in the SD 7.0 specification. Western Digital
announced the first 4 TB SDUC card in 2024, for commercial release in 2025.

Source: Wikipedia, "SD card", citing manufacturer announcements.

Eight years after specification, SDUC remains at the leading edge of
availability. The removable media in the project's motivating incidents is
MBR.

### 3.3 The SD specification constrains MBR layout usefully

The SD Association format specifies a single partition per card, and requires
that the first 32,768 sectors contain only the Master Boot Record and
partition table, so that this region is never rewritten for wear levelling.

Source: discussion citing the SD Association format requirements in the
SdFat project, pull request 406.
<https://github.com/greiman/SdFat/pull/406/>

This is directly usable in M2. On conformant SD media:

* more than one partition is anomalous
* a partition starting before LBA 32,768 is anomalous

Neither is an error in itself, since evidence may be non-conformant or
damaged, but both are conditions worth reporting rather than silently
accepting. This supports the fail-closed requirement in `SAFETY.md` §12.

### 3.4 GPT carries validation MBR does not

GPT headers and partition entry arrays are CRC32-protected, and a backup
table exists at the end of the device. MBR has only a two-byte signature at
`0x1FE`.

This is an argument *for* GPT on forensic grounds: a corrupted GPT can be
detected, and often recovered from the backup. A corrupted MBR frequently
cannot be distinguished from a valid one.

It is not sufficient to reverse the decision, because it applies to media the
project cannot yet obtain or realistically test against, but it is recorded
as a genuine advantage rather than omitted.

---

## 4. Consequences

### Positive

* M2 stays small: one 512-byte structure, four entries, one signature
* No CRC32 implementation decision is required yet
* Covers essentially all media in the motivating incidents
* The SD layout constraints give M2 meaningful anomaly checks

### Negative

* Taphonomy cannot process SDUC media or GPT-partitioned internal drives
* The partition abstraction is designed against one scheme and may require
  revision when GPT is added
* A protective MBR, LBA 0 of a GPT disk, will parse as a single partition of
  type `0xEE` spanning the disk. M2 must detect and report this rather than
  treating it as a real partition, or it will silently misreport every GPT
  disk it is given

The final point is a **requirement on M2**, not merely a consequence. Type
`0xEE` must be recognised and reported as "GPT present, not supported".

---

## 5. Alternatives Considered

### 5.1 MBR and GPT together in M2

Rejected. Doubles M2, introduces a CRC32 dependency or implementation
decision, and designs the abstraction against two schemes before either is
validated. Same reasoning as ADR-0002 §5.3.

### 5.2 GPT first

Rejected. GPT is the more robust format and the better long-term target, but
the project cannot currently obtain SDUC media, and GPT-partitioned internal
drives are outside ADR-0001's initial scope of removable evidence images.
Building first for media that cannot be tested against inverts the project's
evidence-first standard.

---

## 6. Review Trigger

GPT support is required when any of the following occurs:

* SDUC media becomes available to the project
* recovery from internal drives enters scope
* evidence is encountered whose protective MBR indicates GPT
* the FAT32 implementation shows the partition abstraction generalises
  cleanly, making GPT cheap to add

The third trigger is expected to occur in ordinary use. M2's `0xEE` detection
exists so that it is reported clearly rather than silently mishandled.
