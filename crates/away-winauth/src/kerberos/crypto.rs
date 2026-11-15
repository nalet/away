//! How Kerberos seals what it sends (RFC 3961, RFC 3962): a key for each use, made from
//! the key at hand, sealing with AES, and a check with HMAC-SHA1. The cipher and the
//! hash are libraries'. What the standards build from them is written here, and tried
//! against the standards' own examples.

use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use hmac::{Hmac, Mac};
use sha1::{Digest, Sha1};

use crate::Error;

/// How long a block of the cipher is, in bytes.
const BLOCK: usize = 16;
/// How long the check on what is sealed is, in bytes.
pub const CHECK: usize = 12;

/// Which of the two ways of sealing a key is for, as Kerberos numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// AES with a key of 128 bits.
    Aes128 = 17,
    /// AES with a key of 256 bits.
    Aes256 = 18,
}

impl Kind {
    /// The kind a number names, where it names one that is spoken here.
    pub fn of(number: i64) -> Option<Kind> {
        match number {
            17 => Some(Kind::Aes128),
            18 => Some(Kind::Aes256),
            _ => None,
        }
    }

    /// How long a key of this kind is, in bytes.
    pub fn length(self) -> usize {
        match self {
            Kind::Aes128 => 16,
            Kind::Aes256 => 32,
        }
    }
}

/// A key. It can be used, and it can't be printed.
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    kind: Kind,
    bytes: Vec<u8>,
}

impl std::fmt::Debug for Key {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "Key({:?}, ...)", self.kind)
    }
}

impl Key {
    /// A key of this kind. Nothing where the bytes are not as many as the kind takes.
    pub fn new(kind: Kind, bytes: &[u8]) -> Option<Key> {
        (bytes.len() == kind.length()).then(|| Key { kind, bytes: bytes.to_vec() })
    }

    /// Which way of sealing it is for.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The key itself, for the one place that has to write it into a message.
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// The key that both sides have once each knows what the other sent in the exchange
    /// of a sign-in with a certificate (RFC 4556, 3.2.3.1): made from what they share.
    pub fn from_shared(kind: Kind, shared: &[u8]) -> Key {
        let mut bytes = Vec::with_capacity(kind.length() + 20);
        let mut nth = 0u8;
        while bytes.len() < kind.length() {
            let mut hash = Sha1::new();
            hash.update([nth]);
            hash.update(shared);
            bytes.extend_from_slice(&hash.finalize());
            nth += 1;
        }
        bytes.truncate(kind.length());
        Key { kind, bytes }
    }

    fn encrypt(&self, block: &mut [u8; BLOCK]) {
        match self.kind {
            Kind::Aes128 => aes::Aes128::new_from_slice(&self.bytes).expect("a key of 16 bytes").encrypt_block(block.into()),
            Kind::Aes256 => aes::Aes256::new_from_slice(&self.bytes).expect("a key of 32 bytes").encrypt_block(block.into()),
        }
    }

    fn decrypt(&self, block: &mut [u8; BLOCK]) {
        match self.kind {
            Kind::Aes128 => aes::Aes128::new_from_slice(&self.bytes).expect("a key of 16 bytes").decrypt_block(block.into()),
            Kind::Aes256 => aes::Aes256::new_from_slice(&self.bytes).expect("a key of 32 bytes").decrypt_block(block.into()),
        }
    }

    /// The key for one use, made from this one and from what names the use (RFC 3961, 5.1).
    fn derive(&self, constant: &[u8]) -> Key {
        let mut block: [u8; BLOCK] = n_fold(constant, BLOCK).try_into().expect("a block");
        let mut bytes = Vec::with_capacity(self.kind.length() + BLOCK);
        while bytes.len() < self.kind.length() {
            self.encrypt(&mut block);
            bytes.extend_from_slice(&block);
        }
        bytes.truncate(self.kind.length());
        Key { kind: self.kind, bytes }
    }

    /// The key for one use of the three a message has: sealing, the check inside what is
    /// sealed, and a check on its own.
    fn for_use(&self, usage: u32, which: u8) -> Key {
        let mut constant = usage.to_be_bytes().to_vec();
        constant.push(which);
        self.derive(&constant)
    }

    /// Seals a message for one use. The sixteen bytes that begin it are random, and new
    /// for every message.
    pub fn seal(&self, usage: u32, begins_with: [u8; BLOCK], message: &[u8]) -> Vec<u8> {
        let mut plain = begins_with.to_vec();
        plain.extend_from_slice(message);
        let check = mac(&self.for_use(usage, 0x55), &plain);
        let mut sealed = self.for_use(usage, 0xaa).chained(&plain);
        sealed.extend_from_slice(&check);
        sealed
    }

    /// Opens what was sealed for one use, and gives the message.
    pub fn open(&self, usage: u32, sealed: &[u8]) -> Result<Vec<u8>, Error> {
        // sixteen random bytes at least, and the check
        if sealed.len() < BLOCK + CHECK {
            return Err(Error::NotOpened("it is shorter than anything sealed is"));
        }
        let (sealed, check) = sealed.split_at(sealed.len() - CHECK);
        let plain = self.for_use(usage, 0xaa).unchained(sealed);
        // compared without telling by the time it takes where the two differ
        let expected = mac(&self.for_use(usage, 0x55), &plain);
        if expected.iter().zip(check).fold(0u8, |differs, (one, other)| differs | (one ^ other)) != 0 {
            return Err(Error::NotOpened("its check does not hold"));
        }
        Ok(plain[BLOCK..].to_vec())
    }

    /// The check on a message that is not sealed, for one use.
    pub fn check(&self, usage: u32, message: &[u8]) -> [u8; CHECK] {
        mac(&self.for_use(usage, 0x99), message)
    }

    /// Seals with the cipher alone, each block chained to the one before, the last two
    /// swapped and the last cut to what the message has (RFC 3962, 5): what is sealed is
    /// as long as the message.
    fn chained(&self, plain: &[u8]) -> Vec<u8> {
        let mut blocks: Vec<[u8; BLOCK]> = Vec::with_capacity(plain.len().div_ceil(BLOCK));
        let mut before = [0u8; BLOCK];
        for piece in plain.chunks(BLOCK) {
            let mut block = [0u8; BLOCK];
            block[..piece.len()].copy_from_slice(piece);
            for (byte, chained) in block.iter_mut().zip(before) {
                *byte ^= chained;
            }
            self.encrypt(&mut block);
            before = block;
            blocks.push(block);
        }
        let count = blocks.len();
        if count > 1 {
            blocks.swap(count - 2, count - 1);
        }
        let mut sealed = blocks.concat();
        sealed.truncate(plain.len());
        sealed
    }

    /// The other way round. What is given is a block at least.
    fn unchained(&self, sealed: &[u8]) -> Vec<u8> {
        let count = sealed.len().div_ceil(BLOCK);
        let block = |nth: usize| -> [u8; BLOCK] {
            let mut block = [0u8; BLOCK];
            let piece = &sealed[nth * BLOCK..sealed.len().min((nth + 1) * BLOCK)];
            block[..piece.len()].copy_from_slice(piece);
            block
        };
        let mut blocks: Vec<[u8; BLOCK]> = (0..count).map(block).collect();
        if count > 1 {
            // the block before the last arrived last, cut: what was cut off is the end of
            // what the last one opens to
            let last = sealed.len() - (count - 1) * BLOCK;
            let mut opened = blocks[count - 2];
            self.decrypt(&mut opened);
            let mut whole = blocks[count - 1];
            whole[last..].copy_from_slice(&opened[last..]);
            blocks[count - 1] = blocks[count - 2];
            blocks[count - 2] = whole;
        }
        let mut plain = Vec::with_capacity(count * BLOCK);
        let mut before = [0u8; BLOCK];
        for sealed in blocks {
            let mut block = sealed;
            self.decrypt(&mut block);
            for (byte, chained) in block.iter_mut().zip(before) {
                *byte ^= chained;
            }
            before = sealed;
            plain.extend_from_slice(&block);
        }
        plain.truncate(sealed.len());
        plain
    }
}

/// The check on a message: the first twelve bytes of its HMAC with SHA-1.
fn mac(key: &Key, message: &[u8]) -> [u8; CHECK] {
    let mut mac = Hmac::<Sha1>::new_from_slice(&key.bytes).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes()[..CHECK].try_into().expect("twelve bytes")
}

/// Makes bytes of one length from bytes of another, so that every bit of what is given
/// tells in what comes out (RFC 3961, 5.1): the bytes are laid out again and again, each
/// time turned by thirteen bits, and the lot is added up.
pub fn n_fold(given: &[u8], length: usize) -> Vec<u8> {
    let (from, to) = (given.len(), length);
    if from == 0 || to == 0 {
        return vec![0; to];
    }
    let mut common = from * to;
    {
        let (mut one, mut other) = (from, to);
        while other != 0 {
            (one, other) = (other, one % other);
        }
        common /= one;
    }
    let mut folded = vec![0u8; to];
    let mut carried = 0u32;
    for nth in (0..common).rev() {
        // the bit of what is given that is the first of this byte, after the turning
        let first = (((from << 3) - 1) + (((from << 3) + 13) * (nth / from)) + ((from - (nth % from)) << 3)) % (from << 3);
        let pair = (u32::from(given[((from - 1) - (first >> 3)) % from]) << 8) | u32::from(given[(from - (first >> 3)) % from]);
        carried += (pair >> ((first & 7) + 1)) & 0xff;
        carried += u32::from(folded[nth % to]);
        folded[nth % to] = (carried & 0xff) as u8;
        carried >>= 8;
    }
    // what is left over goes round to the end
    if carried != 0 {
        for byte in folded.iter_mut().rev() {
            carried += u32::from(*byte);
            *byte = (carried & 0xff) as u8;
            carried >>= 8;
        }
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(hex: &str) -> Vec<u8> {
        let hex: String = hex.chars().filter(|character| !character.is_whitespace()).collect();
        (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex")).collect()
    }

    #[test]
    fn bytes_are_folded_as_the_standard_folds_them() {
        // RFC 3961, A.1
        let examples: [(&[u8], usize, &str); 11] = [
            (b"012345", 8, "be072631276b1955"),
            (b"password", 7, "78a07b6caf85fa"),
            (b"Rough Consensus, and Running Code", 8, "bb6ed30870b7f0e0"),
            (b"password", 21, "59e4a8ca7c0385c3c37b3f6d2000247cb6e6bd5b3e"),
            (b"MASSACHVSETTS INSTITVTE OF TECHNOLOGY", 24, "db3b0d8f0b061e603282b308a50841229ad798fab9540c1b"),
            (b"Q", 21, "518a54a215a8452a518a54a215a8452a518a54a215"),
            (b"ba", 21, "fb25d531ae8974499f52fd92ea9857c4ba24cf297e"),
            (b"kerberos", 8, "6b65726265726f73"),
            (b"kerberos", 16, "6b65726265726f737b9b5b2b93132b93"),
            (b"kerberos", 21, "8372c236344e5f1550cd0747e15d62ca7a5a3bcea4"),
            (b"kerberos", 32, "6b65726265726f737b9b5b2b93132b935c9bdcdad95c9899c4cae4dee6d6cae4"),
        ];
        for (given, length, folded) in examples {
            assert_eq!(n_fold(given, length), bytes(folded), "{}", String::from_utf8_lossy(given));
        }
        assert_eq!(n_fold(b"", 4), [0; 4]);
    }

    #[test]
    fn the_cipher_chains_its_blocks_and_swaps_the_last_two_as_the_standard_does() {
        // RFC 3962, B: the key "chicken teriyaki", and ever more of one sentence
        let key = Key::new(Kind::Aes128, b"chicken teriyaki").unwrap();
        let sentence = b"I would like the General Gau's Chicken, please, and wonton soup.";
        let examples: [(usize, &str); 6] = [
            (17, "c6353568f2bf8cb4d8a580362da7ff7f97"),
            (31, "fc00783e0efdb2c1d445d4c8eff7ed2297687268d6ecccc0c07b25e25ecfe5"),
            (32, "39312523a78662d5be7fcbcc98ebf5a897687268d6ecccc0c07b25e25ecfe584"),
            (47, "97687268d6ecccc0c07b25e25ecfe584b3fffd940c16a18c1b5549d2f838029e39312523a78662d5be7fcbcc98ebf5"),
            (48, "97687268d6ecccc0c07b25e25ecfe5849dad8bbb96c4cdc03bc103e1a194bbd839312523a78662d5be7fcbcc98ebf5a8"),
            (
                64,
                "97687268d6ecccc0c07b25e25ecfe58439312523a78662d5be7fcbcc98ebf5a84807efe836ee89a526730dbc2f7bc8409dad8bbb96c4cdc03bc103e1a194bbd8",
            ),
        ];
        for (length, sealed) in examples {
            assert_eq!(key.chained(&sentence[..length]), bytes(sealed), "{length} bytes");
            assert_eq!(key.unchained(&bytes(sealed)), &sentence[..length], "{length} bytes, opened");
        }
        // one block is sealed as it is, and every length comes back as it went in
        for length in 16..80 {
            let message: Vec<u8> = (0..length).map(|at| (at * 7) as u8).collect();
            assert_eq!(key.unchained(&key.chained(&message)), message, "{length} bytes");
        }
    }

    #[test]
    fn a_key_for_a_use_is_made_as_the_standard_makes_it() {
        // RFC 3962, B: what a password becomes before the key is made from it, and the key
        let from = Key::new(Kind::Aes128, &bytes("cdedb5281bb2f801565a1122b2563515")).unwrap();
        assert_eq!(from.derive(b"kerberos").bytes, bytes("42263c6e89f4fc28b8df68ee09799f15"));
        let from = Key::new(Kind::Aes256, &bytes("cdedb5281bb2f801565a1122b25635150ad1f7a04bb9f3a333ecc0e2e1f70837")).unwrap();
        assert_eq!(from.derive(b"kerberos").bytes, bytes("fe697b52bc0d3ce14432ba036a92e65bbb52280990a2fa27883998d72af30161"));
        assert_eq!(Kind::of(18).map(Kind::length), Some(32));
        assert_eq!((Kind::of(17), Kind::of(23)), (Some(Kind::Aes128), None));
        assert!(Key::new(Kind::Aes256, &[0; 16]).is_none());
        assert_eq!(format!("{from:?}"), "Key(Aes256, ...)");
    }

    #[test]
    fn it_seals_and_checks_as_another_implementation_of_the_standard_does() {
        // the standard gives no example of a sealed message for AES: these were made with
        // another implementation of it, from the keys of its examples, with sixteen sevens
        // to begin each message
        let key256 = Key::new(Kind::Aes256, &bytes("fe697b52bc0d3ce14432ba036a92e65bbb52280990a2fa27883998d72af30161")).unwrap();
        let key128 = Key::new(Kind::Aes128, &bytes("42263c6e89f4fc28b8df68ee09799f15")).unwrap();
        let forty: Vec<u8> = (0..40).collect();
        let examples: [(&Key, u32, &[u8], &str, &str); 6] = [
            (&key256, 3, b"", "ee5b46856c6a838529ec388647024a5a0e919b812ddf43e5d7cef2e1", "24a0f5c5c899594bf42d38bc"),
            (
                &key256,
                7,
                b"a ticket for a machine",
                "ce51ecbb1236b61607733f2e800fbd327d3e58c03ea97d0e856d6e89bafa3adf3f42d176dee21a91ee16dcf28f9b2676ce9e",
                "5722d19e1327192f46df1d43",
            ),
            (
                &key256,
                11,
                &forty,
                "6d477a3b75221762cc05cc567eddefdf23a1f3ce727827f648348dab0ed237ce3690f861010d6eb31de8e177a57455e89de1b22ef9277f30e510d6ea0ce660812e3928f7",
                "d4c71abe009220a1a89cdb68",
            ),
            (&key128, 3, b"", "171014765bfbcd340bce7bcc156598975d84824c191be546aeb82eee", "3394f97b79171bbac0b7d202"),
            (
                &key128,
                7,
                b"a ticket for a machine",
                "b3879d511573c885f87f7788df9e5ae4458c4efb9c47ea74ae5415447fe834f88891f1532b621483b3cbbda00162044b4210",
                "b66b1cd39ca3d45e8727130a",
            ),
            (
                &key128,
                11,
                &forty,
                "2e7361010f2546d0101d92ac13f9d1d5c77440335e38be65ad43c21c69986eda2fcb6326732735c24f65f0f58a67a418a3d2ca6f0ebc69a8dc958abcf03a29eaa557b82f",
                "ea9d690f05ef4ff70735b43b",
            ),
        ];
        for (key, usage, message, sealed, check) in examples {
            assert_eq!(key.seal(usage, [7; 16], message), bytes(sealed), "{:?}, use {usage}", key.kind());
            assert_eq!(key.open(usage, &bytes(sealed)).as_deref(), Ok(message));
            assert_eq!(key.check(usage, message).to_vec(), bytes(check), "{:?}, use {usage}", key.kind());
        }
    }

    #[test]
    fn what_is_sealed_opens_with_the_same_key_for_the_same_use_and_with_nothing_else() {
        let key = Key::new(Kind::Aes256, &bytes("fe697b52bc0d3ce14432ba036a92e65bbb52280990a2fa27883998d72af30161")).unwrap();
        for length in [0, 1, 15, 16, 17, 100] {
            let message: Vec<u8> = (0..length).map(|at| at as u8).collect();
            let sealed = key.seal(3, [7; 16], &message);
            assert_eq!(sealed.len(), 16 + length + CHECK);
            assert_eq!(key.open(3, &sealed), Ok(message.clone()));
            // another use, another key, a changed byte, and too little
            assert_eq!(key.open(4, &sealed), Err(Error::NotOpened("its check does not hold")));
            let other = Key::new(Kind::Aes256, &[1; 32]).unwrap();
            assert_eq!(other.open(3, &sealed), Err(Error::NotOpened("its check does not hold")));
            let mut changed = sealed.clone();
            changed[5] ^= 1;
            assert_eq!(key.open(3, &changed), Err(Error::NotOpened("its check does not hold")));
            assert!(key.open(3, &sealed[..27]).is_err());
        }
    }
}
