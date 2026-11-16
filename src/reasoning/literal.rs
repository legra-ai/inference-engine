use rdf_reasoning::{Rdf12Datatype, Rdf12LexicalError};
use rdf_structural_model::{LiteralMetadata, ModelError};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, future::Future};

const CHUNK: usize = 4096;

/// One immutable literal's metadata and independently streamed UTF-8 payload.
///
/// Opening this reader is adapter-owned: retain exact source term identity and
/// graph occurrence separately. Metadata access is pure and must not read the
/// lexical payload. Implementations may borrow metadata without allocating it.
pub trait LiteralReader: Send {
    /// Backend payload-read failure.
    type Error: Error + Send + Sync + 'static;
    /// Borrow standard metadata, including original language spelling/direction.
    fn metadata(&self) -> LiteralMetadata<&str>;
    /// Fill at most the supplied buffer, returning zero only at successful EOF.
    ///
    /// UTF-8 scalars may straddle reads. Errors and cancellation must never be
    /// disguised as EOF; drop must release/cancel the underlying read.
    fn read(
        &mut self,
        output: &mut [u8],
    ) -> impl Future<Output = Result<usize, Self::Error>> + Send;
    /// Whether the enclosing inference attempt was cancelled or superseded.
    fn is_cancelled(&self) -> bool;
}

/// Successful lexical inspection, without allocating or normalizing the literal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralReport {
    /// Recognized default datatype; `None` means an encountered opaque datatype.
    /// Opaque does not mean ill-typed, nor does it extend the recognized map.
    pub datatype: Option<Rdf12Datatype>,
    /// Exact lexical UTF-8 byte count, after verifying successful EOF.
    pub bytes: u64,
}

/// Bounded lexical inspection under the three mandatory RDF 1.2 datatypes.
///
/// Uses shared structural metadata checks and `rdf-reasoning` lexical semantics.
/// Unknown encountered datatypes remain opaque. This does not implement datatype
/// value comparison or graph-wide consistency and never rewrites a source term.
pub struct Rdf12LiteralValidator {
    maximum: u64,
}

impl Rdf12LiteralValidator {
    /// Set a strict byte budget, including opaque datatype payloads. A zero
    /// budget permits only an empty lexical form, verified through an EOF probe.
    pub const fn new(maximum: u64) -> Self {
        Self { maximum }
    }

    /// Validate metadata, then stream the lexical form using 4099 bytes of scratch.
    ///
    /// No whole literal is retained. At most three incomplete UTF-8 bytes carry
    /// between reads. An exact byte budget permits the final EOF probe; exceeding
    /// it fails. Callers separately budget total attempt work across literals.
    ///
    /// # Errors
    /// Invalid standard metadata, invalid UTF-8, a recognized lexical violation,
    /// cancellation, resource exhaustion or a backend error abort inspection.
    /// A lexical violation is semantic evidence about this literal; it does not
    /// certify graph inconsistency or authorize publishing a partial inference.
    pub async fn validate<R: LiteralReader>(
        &self,
        reader: &mut R,
    ) -> Result<LiteralReport, LiteralError<R::Error>> {
        Self::check_cancelled(reader)?;
        let datatype = {
            let metadata = reader.metadata();
            metadata
                .validate_standard()
                .map_err(LiteralError::Metadata)?;
            Rdf12Datatype::recognize_iri(metadata.datatype().as_str())
        };
        // bounded: one 4096-byte read plus at most three incomplete UTF-8 bytes.
        let mut buffer = [0_u8; CHUNK + 3];
        let mut pending = 0;
        let mut bytes = 0_u64;
        let mut offset = 0_u64;
        let mut read_steps = 0_u64;
        loop {
            Self::check_cancelled(reader)?;
            let remaining = self.maximum - bytes;
            let capacity = remaining.saturating_add(1).min(CHUNK as u64) as usize;
            let count = reader
                .read(&mut buffer[pending..pending + capacity])
                .await
                .map_err(LiteralError::Storage)?;
            Self::check_cancelled(reader)?;
            if count > capacity {
                return Err(LiteralError::ReadContract);
            }
            // Bounded scheduling counter; aggregate byte accounting stays exact below.
            read_steps = (read_steps + 1) % crate::cooperative::QUANTUM;
            crate::cooperative::checkpoint(read_steps).await;
            Self::check_cancelled(reader)?;
            if count as u64 > remaining {
                return Err(LiteralError::ByteLimit);
            }
            bytes = bytes
                .checked_add(count as u64)
                .ok_or(LiteralError::LengthOverflow)?;
            if count == 0 {
                return if pending == 0 {
                    Ok(LiteralReport { datatype, bytes })
                } else {
                    Err(LiteralError::Utf8 {
                        byte_offset: offset,
                    })
                };
            }
            let length = pending + count;
            let valid = match std::str::from_utf8(&buffer[..length]) {
                Ok(_) => length,
                Err(error) if error.error_len().is_none() => error.valid_up_to(),
                Err(error) => {
                    let byte_offset = offset
                        .checked_add(error.valid_up_to() as u64)
                        .ok_or(LiteralError::LengthOverflow)?;
                    return Err(LiteralError::Utf8 { byte_offset });
                }
            };
            let fragment = std::str::from_utf8(&buffer[..valid])
                .expect("UTF-8 decoder's valid prefix must be UTF-8");
            if let Some(datatype) = datatype {
                datatype
                    .validate_lexical(fragment)
                    .map_err(|error| LiteralError::Lexical {
                        fragment_offset: offset,
                        error,
                    })?;
            }
            offset = offset
                .checked_add(valid as u64)
                .ok_or(LiteralError::LengthOverflow)?;
            buffer.copy_within(valid..length, 0);
            pending = length - valid;
        }
    }

    fn check_cancelled<R: LiteralReader>(reader: &R) -> Result<(), LiteralError<R::Error>> {
        if reader.is_cancelled() {
            Err(LiteralError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Failed literal inspection; backend failures remain distinct from semantics.
#[derive(Debug)]
pub enum LiteralError<E> {
    /// Payload access failed.
    Storage(E),
    /// Standard datatype/facet pairing is invalid or an extension is unsupported.
    Metadata(ModelError),
    /// Malformed UTF-8 or an incomplete scalar at EOF, at a global byte offset.
    Utf8 {
        /// Start of the malformed byte sequence in the lexical payload.
        byte_offset: u64,
    },
    /// Shared recognized-datatype lexical violation, without changing its witness.
    Lexical {
        /// Global start of the fragment; add the shared error's relative offset.
        fragment_offset: u64,
        /// Shared semantics' invalid character and fragment-relative byte offset.
        error: Rdf12LexicalError,
    },
    /// The literal exceeded its explicit lexical byte budget.
    ByteLimit,
    /// A reader reported more bytes than the supplied buffer could hold.
    ReadContract,
    /// Lexical byte accounting exceeded the supported u64 range.
    LengthOverflow,
    /// Caller cancelled or superseded the enclosing attempt.
    Cancelled,
}

impl<E: fmt::Display> fmt::Display for LiteralError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "literal storage failed: {error}"),
            Self::Metadata(error) => write!(f, "unsupported standard literal metadata: {error}"),
            Self::Utf8 { byte_offset } => write!(f, "invalid lexical UTF-8 at byte {byte_offset}"),
            Self::Lexical {
                fragment_offset,
                error,
            } => write!(
                f,
                "literal fragment starting at byte {fragment_offset}: {error}"
            ),
            Self::ByteLimit => f.write_str("literal lexical byte limit exceeded"),
            Self::ReadContract => f.write_str("literal reader exceeded its supplied buffer"),
            Self::LengthOverflow => f.write_str("literal byte count overflow"),
            Self::Cancelled => f.write_str("literal inspection cancelled"),
        }
    }
}

impl<E: Error + 'static> Error for LiteralError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Metadata(error) => Some(error),
            Self::Lexical { error, .. } => Some(error),
            _ => None,
        }
    }
}
