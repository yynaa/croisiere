//! Values used by the counter domain.
//!
//! `CounterName` identifies a counter, while `CounterData` holds its count.
//! Both compare and hash by their contents; neither has an identity of its own.

use croisiere::domain::ValueObject;

/// Domain identity of a counter, independent of its storage identifier.
///
/// Two names containing the same string are equal. The entity uses this value
/// to identify a counter; the storage UUID belongs to infrastructure.
///
/// Conversion rejects empty strings and otherwise preserves the supplied string
/// without normalization. The default name is `"-"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct CounterName(String);

impl CounterName {
    /// Borrows the validated name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the value object and returns its name.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl Default for CounterName {
    fn default() -> Self {
        Self("-".to_owned())
    }
}

impl TryFrom<String> for CounterName {
    type Error = CounterNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() {
            return Err(CounterNameError::EmptyName);
        }
        Ok(Self(name))
    }
}

/// A name failed the counter domain's validation rules.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CounterNameError {
    /// A counter name must contain at least one character.
    #[error("a counter name cannot be empty")]
    EmptyName,
}

/// Counter state attached to the counter aggregate.
///
/// The default count is zero. The aggregate's increment and decrement functions
/// change this state and enforce the rule against decrementing below zero.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct CounterData {
    /// Current count, represented as a nonnegative integer.
    pub counter: u64,
}
