use std::fmt::Display;

use crate::errors::PgmqError;
/// A string that is known to be formed of only ASCII alphanumeric or an underscore;
#[derive(Clone, Copy)]
pub struct CheckedName<'a>(&'a str);

impl<'a> CheckedName<'a> {
    /// Accepts `input` as a CheckedName if it is a valid queue identifier
    pub fn new(input: &'a str) -> Result<Self, PgmqError> {
        check_input(input)?;

        Ok(Self(input))
    }
}

impl AsRef<str> for CheckedName<'_> {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl Display for CheckedName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// panics if input is invalid. otherwise does nothing.
pub fn check_input(input: &str) -> Result<(), PgmqError> {
    // Docs:
    // https://www.postgresql.org/docs/current/sql-syntax-lexical.html#SQL-SYNTAX-IDENTIFIERS

    // Default value of `NAMEDATALEN`, set in `src/include/pg_config_manual.h`
    const NAMEDATALEN: usize = 64;

    // The maximum length of an identifier.
    // Longer names can be used in commands, but they'll be truncated
    const MAX_IDENTIFIER_LEN: usize = NAMEDATALEN - 1;
    const BIGGEST_CONCAT: &str = "archived_at_idx_";

    // The max length of the name of a PGMQ queue, considering that the biggest
    // postgres identifier created by PGMQ is the index on archived_at
    const MAX_PGMQ_QUEUE_LEN: usize = MAX_IDENTIFIER_LEN - BIGGEST_CONCAT.len();

    let is_short_enough = input.len() <= MAX_PGMQ_QUEUE_LEN;
    let has_valid_characters = input
        .as_bytes()
        .iter()
        .all(|&c| c.is_ascii_alphanumeric() || c == b'_');
    let valid = is_short_enough && has_valid_characters;
    match valid {
        true => Ok(()),
        false => Err(PgmqError::InvalidQueueName {
            name: input.to_owned(),
        }),
    }
}
