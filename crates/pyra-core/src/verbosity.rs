#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum Verbosity {
    #[default]
    Normal,
    Verbose,
}

impl Verbosity {
    pub fn from_occurrences(occurrences: u8) -> Self {
        if occurrences == 0 {
            Self::Normal
        } else {
            Self::Verbose
        }
    }

    pub fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}
