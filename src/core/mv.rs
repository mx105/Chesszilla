#![allow(dead_code)]

use crate::core::types::{PieceKind, Square};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Move(pub u32);

impl Move {
    const FROM_MASK: u32 = 0b11_1111;
    const TO_SHIFT: u32 = 6;
    const TO_MASK: u32 = 0b11_1111 << Self::TO_SHIFT;
    const PROMOTION_SHIFT: u32 = 12;
    const PROMOTION_MASK: u32 = 0b111 << Self::PROMOTION_SHIFT;

    const CAPTURE_FLAG: u32 = 1 << 15;
    const DOUBLE_PAWN_PUSH_FLAG: u32 = 1 << 16;
    const EN_PASSANT_FLAG: u32 = 1 << 17;
    const CASTLING_FLAG: u32 = 1 << 18;

    pub const fn new(from: Square, to: Square) -> Self {
        Move((from.0 as u32) | ((to.0 as u32) << Self::TO_SHIFT))
    }

    pub const fn with_promotion(from: Square, to: Square, promoted: PieceKind) -> Self {
        let code = match promoted {
            PieceKind::Knight => 1,
            PieceKind::Bishop => 2,
            PieceKind::Rook => 3,
            PieceKind::Queen => 4,
            PieceKind::Pawn | PieceKind::King => 0,
        };
        Move(Self::new(from, to).0 | (code << Self::PROMOTION_SHIFT))
    }

    pub const fn from(self) -> Square {
        Square((self.0 & Self::FROM_MASK) as u8)
    }

    pub const fn to(self) -> Square {
        Square(((self.0 & Self::TO_MASK) >> Self::TO_SHIFT) as u8)
    }

    pub const fn promotion(self) -> Option<PieceKind> {
        match (self.0 & Self::PROMOTION_MASK) >> Self::PROMOTION_SHIFT {
            1 => Some(PieceKind::Knight),
            2 => Some(PieceKind::Bishop),
            3 => Some(PieceKind::Rook),
            4 => Some(PieceKind::Queen),
            _ => None,
        }
    }

    pub const fn with_capture(self) -> Self {
        Move(self.0 | Self::CAPTURE_FLAG)
    }

    pub const fn with_double_pawn_push(self) -> Self {
        Move(self.0 | Self::DOUBLE_PAWN_PUSH_FLAG)
    }

    pub const fn with_en_passant(self) -> Self {
        Move(self.0 | Self::EN_PASSANT_FLAG | Self::CAPTURE_FLAG)
    }

    pub const fn with_castling(self) -> Self {
        Move(self.0 | Self::CASTLING_FLAG)
    }

    pub const fn is_capture(self) -> bool {
        self.0 & Self::CAPTURE_FLAG != 0
    }

    pub const fn is_double_pawn_push(self) -> bool {
        self.0 & Self::DOUBLE_PAWN_PUSH_FLAG != 0
    }

    pub const fn is_en_passant(self) -> bool {
        self.0 & Self::EN_PASSANT_FLAG != 0
    }

    pub const fn is_castling(self) -> bool {
        self.0 & Self::CASTLING_FLAG != 0
    }

    pub fn to_uci(self) -> String {
        let mut text = format!("{}{}", self.from().to_uci(), self.to().to_uci());
        if let Some(promotion) = self.promotion() {
            let promotion = match promotion {
                PieceKind::Knight => 'n',
                PieceKind::Bishop => 'b',
                PieceKind::Rook => 'r',
                PieceKind::Queen => 'q',
                PieceKind::Pawn | PieceKind::King => unreachable!("invalid promotion piece"),
            };
            text.push(promotion);
        }
        text
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn normal_move_round_trip() {
        let mv = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3));

        assert_eq!(mv.from(), Square::from_file_rank(4, 1));
        assert_eq!(mv.to(), Square::from_file_rank(4, 3));
        assert_eq!(mv.promotion(), None);
        assert!(!mv.is_capture());
        assert!(!mv.is_double_pawn_push());
        assert!(!mv.is_en_passant());
        assert!(!mv.is_castling());
    }

    #[test]
    fn promotion_round_trip() {
        let mv = Move::with_promotion(
            Square::from_file_rank(0, 6),
            Square::from_file_rank(0, 7),
            PieceKind::Queen,
        );

        assert_eq!(mv.from(), Square::from_file_rank(0, 6));
        assert_eq!(mv.to(), Square::from_file_rank(0, 7));
        assert_eq!(mv.promotion(), Some(PieceKind::Queen));
    }

    #[test]
    fn special_flags_round_trip() {
        let base = Move::new(Square::from_file_rank(4, 4), Square::from_file_rank(5, 5));

        assert!(base.with_capture().is_capture());
        assert!(base.with_double_pawn_push().is_double_pawn_push());
        assert!(base.with_en_passant().is_en_passant());
        assert!(base.with_en_passant().is_capture());
        assert!(base.with_castling().is_castling());
    }

    #[test]
    fn flags_do_not_corrupt_move_fields() {
        let mv = Move::with_promotion(
            Square::from_file_rank(6, 6),
            Square::from_file_rank(7, 7),
            PieceKind::Knight,
        )
        .with_capture()
        .with_double_pawn_push()
        .with_en_passant()
        .with_castling();

        assert_eq!(mv.from(), Square::from_file_rank(6, 6));
        assert_eq!(mv.to(), Square::from_file_rank(7, 7));
        assert_eq!(mv.promotion(), Some(PieceKind::Knight));
        assert!(mv.is_capture());
        assert!(mv.is_double_pawn_push());
        assert!(mv.is_en_passant());
        assert!(mv.is_castling());
    }

    #[test]
    fn uci_formats_quiet_moves() {
        let mv = Move::new(Square::from_file_rank(4, 1), Square::from_file_rank(4, 3));

        assert_eq!(mv.to_uci(), "e2e4");
    }

    #[test]
    fn uci_formats_castling_moves() {
        let kingside =
            Move::new(Square::from_file_rank(4, 0), Square::from_file_rank(6, 0)).with_castling();
        let queenside =
            Move::new(Square::from_file_rank(4, 7), Square::from_file_rank(2, 7)).with_castling();

        assert_eq!(kingside.to_uci(), "e1g1");
        assert_eq!(queenside.to_uci(), "e8c8");
    }

    #[test]
    fn uci_formats_promotion_moves() {
        let queen = Move::with_promotion(
            Square::from_file_rank(0, 6),
            Square::from_file_rank(0, 7),
            PieceKind::Queen,
        );
        let knight = Move::with_promotion(
            Square::from_file_rank(1, 6),
            Square::from_file_rank(2, 7),
            PieceKind::Knight,
        )
        .with_capture();

        assert_eq!(queen.to_uci(), "a7a8q");
        assert_eq!(knight.to_uci(), "b7c8n");
    }
}
