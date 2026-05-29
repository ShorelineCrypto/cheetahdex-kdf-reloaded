use crypto::dhash256;
use primitives::hash::H256;

/// Computes the Bitcoin merkle root over a slice of leaf hashes.
///
/// Reference: <https://en.bitcoin.it/wiki/Protocol_documentation#Merkle_Trees>.
/// Single-element inputs return the leaf itself; odd-sized rows duplicate the
/// final element before pairing, matching Bitcoin Core's behaviour.
pub fn merkle_root<T>(leaves: &[T]) -> H256
where
    T: AsRef<H256>,
{
    debug_assert!(!leaves.is_empty(), "merkle_root: empty input is undefined");

    let mut current: Vec<H256> = leaves.iter().map(|h| *h.as_ref()).collect();
    while current.len() > 1 {
        let mut next = Vec::with_capacity(current.len().div_ceil(2));
        let mut idx = 0;
        while idx < current.len() {
            let left = &current[idx];
            let right = if idx + 1 < current.len() {
                &current[idx + 1]
            } else {
                left
            };
            next.push(merkle_node_hash(left, right));
            idx += 2;
        }
        current = next;
    }
    current[0]
}

/// Hashes one merkle-tree internal node from its two children using DSHA-256.
pub fn merkle_node_hash<T>(left: T, right: T) -> H256
where
    T: AsRef<H256>,
{
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&**left.as_ref());
    buf[32..].copy_from_slice(&**right.as_ref());
    dhash256(&buf)
}

#[cfg(test)]
mod tests {
    use super::merkle_root;
    use primitives::hash::H256;

    // Bitcoin block 80_000 — txids:
    //   c06fbab289f723c6261d3030ddb6be121f7d2508d77862bb1e484f5cd7f92b25
    //   5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2
    // Expected root from the on-chain block header.
    #[test]
    fn merkle_root_two_leaves_matches_block_80000() {
        let a = H256::from_reversed_str("c06fbab289f723c6261d3030ddb6be121f7d2508d77862bb1e484f5cd7f92b25");
        let b = H256::from_reversed_str("5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2");
        let expected = H256::from_reversed_str("8fb300e3fdb6f30a4c67233b997f99fdd518b968b9a3fd65857bfe78b2600719");
        assert_eq!(merkle_root(&[a, b]), expected);
        assert_eq!(merkle_root(&[&a, &b]), expected);
    }
}
