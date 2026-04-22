#![allow(dead_code)]

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Bitboard(pub u64);

impl Bitboard {
    pub const EMPTY: Self = Bitboard(0);

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, square: Square) -> bool {
        (self.0 & (1u64 << square.0)) != 0
    }

    pub const fn set(&mut self, square: Square) {
        self.0 |= 1u64 << square.0;
    }

    pub const fn clear(&mut self, square: Square) {
        self.0 &= !(1u64 << square.0);
    }

    pub const fn without(self, square: Square) -> Self {
        Bitboard(self.0 & !(1u64 << square.0))
    }

    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    pub fn pop_lsb(&mut self) -> Option<Square> {
        if self.0 == 0 {
            return None;
        }
        let lsb = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Some(Square(lsb))
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    pub const fn idx(self) -> usize {
        self as usize
    }
    pub const fn opposit(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PieceKind {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
}

impl PieceKind {
    pub const fn idx(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Piece {
    WhitePawn = 0,
    WhiteKnight,
    WhiteBishop,
    WhiteRook,
    WhiteQueen,
    WhiteKing,
    BlackPawn,
    BlackKnight,
    BlackBishop,
    BlackRook,
    BlackQueen,
    BlackKing,
}

impl Piece {
    pub const fn idx(self) -> usize {
        self as usize
    }

    pub const fn color(self) -> Color {
        if (self as u8) < 6 {
            Color::White
        } else {
            Color::Black
        }
    }

    pub const fn kind(self) -> PieceKind {
        match (self as u8) % 6 {
            0 => PieceKind::Pawn,
            1 => PieceKind::Knight,
            2 => PieceKind::Bishop,
            3 => PieceKind::Rook,
            4 => PieceKind::Queen,
            _ => PieceKind::King,
        }
    }

    pub const fn from(color: Color, kind: PieceKind) -> Self {
        use Color::*;
        use PieceKind::*;
        match (color, kind) {
            (White, Pawn) => Piece::WhitePawn,
            (White, Knight) => Piece::WhiteKnight,
            (White, Bishop) => Piece::WhiteBishop,
            (White, Rook) => Piece::WhiteRook,
            (White, Queen) => Piece::WhiteQueen,
            (White, King) => Piece::WhiteKing,
            (Black, Pawn) => Piece::BlackPawn,
            (Black, Knight) => Piece::BlackKnight,
            (Black, Bishop) => Piece::BlackBishop,
            (Black, Rook) => Piece::BlackRook,
            (Black, Queen) => Piece::BlackQueen,
            (Black, King) => Piece::BlackKing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct Square(pub u8);

impl Square {
    pub const fn idx(self) -> usize {
        self.0 as usize
    }

    pub const fn file(self) -> u8 {
        self.0 & 7
    }

    pub const fn rank(self) -> u8 {
        self.0 >> 3
    }

    pub const fn from_file_rank(file: u8, rank: u8) -> Self {
        Square(rank * 8 + file)
    }

    pub fn from_uci(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 2 {
            return None;
        }

        let file = match bytes[0] {
            b'a'..=b'h' => bytes[0] - b'a',
            _ => return None,
        };
        let rank = match bytes[1] {
            b'1'..=b'8' => bytes[1] - b'1',
            _ => return None,
        };

        Some(Square::from_file_rank(file, rank))
    }

    pub fn to_uci(self) -> String {
        let file = (b'a' + self.file()) as char;
        let rank = (b'1' + self.rank()) as char;
        format!("{file}{rank}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_idx() {
        let score = [100, 200];
        assert_eq!(score[Color::White.idx()], 100);
        assert_eq!(score[Color::Black.idx()], 200);
    }

    #[test]
    fn test_opposit() {
        assert_eq!(Color::White.opposit(), Color::Black);
        assert_eq!(Color::Black.opposit(), Color::White);
    }

    #[test]
    fn test_piece_color() {
        assert_eq!(Piece::BlackRook.color(), Color::Black);
        assert_eq!(Piece::WhiteQueen.color(), Color::White);
    }

    #[test]
    fn test_piece_kind() {
        assert_eq!(Piece::BlackRook.kind(), PieceKind::Rook);
        assert_eq!(Piece::WhiteQueen.kind(), PieceKind::Queen);
    }

    #[test]
    fn test_from_color_kind() {
        assert_eq!(Piece::from(Color::Black, PieceKind::King), Piece::BlackKing);
    }

    #[test]
    fn square_uci_round_trip() {
        for text in ["a1", "e4", "h8"] {
            let square = Square::from_uci(text).unwrap();

            assert_eq!(square.to_uci(), text);
            assert_eq!(Square::from_uci(&square.to_uci()), Some(square));
        }
    }

    #[test]
    fn square_uci_rejects_invalid_text() {
        for text in ["", "a", "a11", "i1", "a0", "A1", "h9"] {
            assert_eq!(Square::from_uci(text), None);
        }
    }
}
