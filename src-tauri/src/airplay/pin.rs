// PIN pairing (pair-pin-start / pair-setup-pin): Apple's variant of SRP-6a (SHA-1, 2048-bit group,
// 40-byte session key), then the iPad's and this PC's Ed25519 keys are exchanged with AES-GCM.
// As in UxPlay (GPL-3.0).
use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::aes::Aes128;
use aes_gcm::AesGcm;
use num_bigint::BigUint;
use sha1::{Digest, Sha1};
use sha2::Sha512;

type Gcm16 = AesGcm<Aes128, aes_gcm::aead::consts::U16>;
const N_HEX: &str = "AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4FF747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DBFBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73";

fn n() -> BigUint { BigUint::parse_bytes(N_HEX.as_bytes(), 16).unwrap() }
fn h(parts: &[&[u8]]) -> [u8; 20] { let mut d = Sha1::new(); for p in parts { d.update(p); } d.finalize().into() }
fn pad(x: &BigUint, len: usize) -> Vec<u8> { let b = x.to_bytes_be(); let mut v = vec![0u8; len.saturating_sub(b.len())]; v.extend(b); v }
fn rand(n: usize) -> Vec<u8> { let mut b = vec![0u8; n]; let _ = getrandom::getrandom(&mut b); b }

pub struct Srp { user: String, salt: Vec<u8>, v: BigUint, b: BigUint, big_b: BigUint }

impl Srp {
    /// first step: the iPad sends its device id, gets salt and our public value B
    pub fn new(user: &str, pin: &str) -> (Srp, Vec<u8>, Vec<u8>) {
        let (nn, g) = (n(), BigUint::from(2u32));
        let mut salt = rand(16);
        salt[0] |= 1; // no leading zero: both sides use the same bytes
        let x = BigUint::from_bytes_be(&h(&[&salt, &h(&[user.as_bytes(), b":", pin.as_bytes()])]));
        let v = g.modpow(&x, &nn);
        let b = BigUint::from_bytes_be(&rand(32));
        let k = BigUint::from_bytes_be(&h(&[&nn.to_bytes_be(), &pad(&g, 256)]));
        let big_b = (&k * &v + g.modpow(&b, &nn)) % &nn;
        let bb = big_b.to_bytes_be();
        (Srp { user: user.to_string(), salt: salt.clone(), v, b, big_b }, salt, bb)
    }
    /// second step: checks the iPad's proof (so the PIN was right); gives our proof and the session key
    pub fn verify(&self, a_bytes: &[u8], m1: &[u8]) -> Option<([u8; 20], [u8; 40])> {
        let nn = n();
        let g = BigUint::from(2u32);
        let a = BigUint::from_bytes_be(a_bytes);
        if (&a % &nn) == BigUint::from(0u32) { return None; }
        let u = BigUint::from_bytes_be(&h(&[&pad(&a, 256), &pad(&self.big_b, 256)]));
        let s = (&a * self.v.modpow(&u, &nn)).modpow(&self.b, &nn);
        let sb = s.to_bytes_be();
        let mut key = [0u8; 40];
        key[..20].copy_from_slice(&h(&[&sb, &[0, 0, 0, 0]]));
        key[20..].copy_from_slice(&h(&[&sb, &[0, 0, 0, 1]]));
        let (hn, hg) = (h(&[&nn.to_bytes_be()]), h(&[&g.to_bytes_be()]));
        let mut hx = [0u8; 20];
        for i in 0..20 { hx[i] = hn[i] ^ hg[i]; }
        let m = h(&[&hx, &h(&[self.user.as_bytes()]), &BigUint::from_bytes_be(&self.salt).to_bytes_be(), &a.to_bytes_be(), &self.big_b.to_bytes_be(), &key]);
        if m1.len() != 20 || m[..] != m1[..] { return None; }
        Some((h(&[&a.to_bytes_be(), &m, &key]), key))
    }
}

fn gcm_keys(key: &[u8; 40]) -> ([u8; 16], [u8; 16]) {
    let k = Sha512::new_with_prefix(b"Pair-Setup-AES-Key").chain_update(key).finalize();
    let i = Sha512::new_with_prefix(b"Pair-Setup-AES-IV").chain_update(key).finalize();
    let (mut a, mut b) = ([0u8; 16], [0u8; 16]);
    a.copy_from_slice(&k[..16]);
    b.copy_from_slice(&i[..16]);
    (a, b)
}

/// third step: the iPad's Ed25519 key comes encrypted; we answer with ours. -> (iPad key, our epk, our tag)
pub fn confirm(key: &[u8; 40], epk: &[u8], tag: &[u8], ours: &[u8; 32]) -> Option<([u8; 32], Vec<u8>, Vec<u8>)> {
    if epk.len() != 32 || tag.len() != 16 { return None; }
    let (k, mut iv) = gcm_keys(key);
    let gcm = Gcm16::new(&k.into());
    iv[15] = iv[15].wrapping_add(1);
    let mut client = epk.to_vec();
    gcm.decrypt_in_place_detached((&iv).into(), b"", &mut client, tag.into()).ok()?;
    iv[15] = iv[15].wrapping_add(1);
    let mut mine = ours.to_vec();
    let t = gcm.encrypt_in_place_detached((&iv).into(), b"", &mut mine).ok()?;
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&client);
    Some((pk, mine, t.to_vec()))
}

/// a random PIN, always 4 digits
pub fn new_pin() -> String {
    let b = rand(2);
    format!("{:04}", 1000 + (u16::from_le_bytes([b[0], b[1]]) % 9000))
}

#[cfg(test)]
mod tests {
    use super::*;
    // plays the iPad's side of SRP with the same formulas and checks both proofs and the key exchange
    #[test]
    fn srp_and_gcm() {
        let (user, pin) = ("AA:BB:CC:DD:EE:FF", "4711");
        let (srv, salt, bb) = Srp::new(user, pin);
        let (nn, g) = (n(), BigUint::from(2u32));
        let a = BigUint::from_bytes_be(&rand(32));
        let big_a = g.modpow(&a, &nn);
        let big_b = BigUint::from_bytes_be(&bb);
        let k = BigUint::from_bytes_be(&h(&[&nn.to_bytes_be(), &pad(&g, 256)]));
        let u = BigUint::from_bytes_be(&h(&[&pad(&big_a, 256), &pad(&big_b, 256)]));
        let x = BigUint::from_bytes_be(&h(&[&salt, &h(&[user.as_bytes(), b":", pin.as_bytes()])]));
        let gx = (&k * g.modpow(&x, &nn)) % &nn;
        let base = (&big_b + &nn - gx) % &nn;
        let s = base.modpow(&(&a + &u * &x), &nn);
        let sb = s.to_bytes_be();
        let mut key = [0u8; 40];
        key[..20].copy_from_slice(&h(&[&sb, &[0, 0, 0, 0]]));
        key[20..].copy_from_slice(&h(&[&sb, &[0, 0, 0, 1]]));
        let (hn, hg) = (h(&[&nn.to_bytes_be()]), h(&[&g.to_bytes_be()]));
        let hx: Vec<u8> = hn.iter().zip(hg).map(|(p, q)| p ^ q).collect();
        let m1 = h(&[&hx, &h(&[user.as_bytes()]), &salt, &big_a.to_bytes_be(), &bb, &key]);
        let (m2, skey) = srv.verify(&big_a.to_bytes_be(), &m1).expect("right PIN must pass");
        assert_eq!(skey, key);
        assert_eq!(m2, h(&[&big_a.to_bytes_be(), &m1, &key]));
        // wrong PIN fails
        let (srv2, _, _) = Srp::new(user, "0000");
        assert!(srv2.verify(&big_a.to_bytes_be(), &m1).is_none());
        // key exchange: client encrypts with iv+1, server answers with iv+2
        let (gk, mut iv) = gcm_keys(&key);
        let gcm = Gcm16::new(&gk.into());
        iv[15] += 1;
        let mut epk = [7u8; 32].to_vec();
        let tag = gcm.encrypt_in_place_detached((&iv).into(), b"", &mut epk).unwrap();
        let (client, mine, t) = confirm(&key, &epk, &tag, &[9u8; 32]).expect("tag must check out");
        assert_eq!(client, [7u8; 32]);
        iv[15] += 1;
        let mut back = mine.clone();
        gcm.decrypt_in_place_detached((&iv).into(), b"", &mut back, t.as_slice().into()).unwrap();
        assert_eq!(back, [9u8; 32].to_vec());
        assert_eq!(new_pin().len(), 4);
    }
}
