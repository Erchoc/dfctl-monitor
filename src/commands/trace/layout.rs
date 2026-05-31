#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceTier {
    TooSmall,
    /// Phone SSH: single-column span list, summary on its own page.
    Phone,
    /// Narrow desktop: tree + waterfall, no sidebar.
    Compact,
    /// Default desktop: tree + waterfall.
    Wide,
    /// Very wide: tree + waterfall + always-on summary sidebar.
    WideSidebar,
    /// Portrait monitor: waterfall on top, summary below.
    Portrait,
}

impl TraceTier {
    pub fn from_size(w: u16, h: u16) -> Self {
        if w < 60 || h < 18 {
            return Self::TooSmall;
        }
        if w < 100 || h < 24 {
            return Self::Phone;
        }
        if h > w && h > 60 {
            return Self::Portrait;
        }
        match w {
            0..=139 => Self::Compact,
            140..=199 => Self::Wide,
            _ => Self::WideSidebar,
        }
    }

    pub fn is_phone(self) -> bool {
        matches!(self, Self::Phone)
    }
}
