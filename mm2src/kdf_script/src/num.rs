// Bitcoin script "numeric" encoding — sign-magnitude little-endian
// integers used by `OP_*` arithmetic. Kept internal: nothing outside
// this crate needs the type. It exists only to back `Builder::push_num`
// which our test suite uses via `Into`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Num(i64);

macro_rules! num_from_int {
    ($($t:ty),* $(,)?) => {
        $(impl From<$t> for Num { fn from(v: $t) -> Self { Num(v as i64) } })*
    };
}
num_from_int!(i32, i64, u8, u32, usize);

impl From<bool> for Num {
    fn from(b: bool) -> Self { Num(if b { 1 } else { 0 }) }
}

impl Num {
    /// Encode according to the Bitcoin script numeric convention:
    /// sign-magnitude little-endian, with the most-significant bit of
    /// the last byte indicating sign. A zero value encodes as the
    /// empty byte string.
    pub fn to_bytes(self) -> Vec<u8> {
        let mut value = self.0;
        if value == 0 {
            return Vec::new();
        }
        let negative = value < 0;
        if negative {
            value = -value;
        }
        let mut out = Vec::with_capacity(9);
        while value != 0 {
            out.push((value & 0xff) as u8);
            value >>= 8;
        }
        // If the top bit is already set we need an extra byte to
        // carry the sign without altering magnitude.
        if out.last().copied().unwrap_or(0) & 0x80 != 0 {
            out.push(if negative { 0x80 } else { 0 });
        } else if negative {
            *out.last_mut().unwrap() |= 0x80;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::Num;

    #[test]
    fn encodes_basic_values() {
        assert_eq!(Num::from(0).to_bytes(), Vec::<u8>::new());
        assert_eq!(Num::from(1).to_bytes(), vec![0x01]);
        assert_eq!(Num::from(-1).to_bytes(), vec![0x81]);
        assert_eq!(Num::from(127).to_bytes(), vec![0x7f]);
        assert_eq!(Num::from(128).to_bytes(), vec![0x80, 0x00]);
        assert_eq!(Num::from(-128).to_bytes(), vec![0x80, 0x80]);
        assert_eq!(Num::from(256).to_bytes(), vec![0x00, 0x01]);
    }
}
