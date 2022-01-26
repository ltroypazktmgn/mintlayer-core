pub mod rschnorr;
pub mod signature;

use parity_scale_codec_derive::{Decode as DecodeDer, Encode as EncodeDer};
use rand::SeedableRng;
pub use signature::Signature;

use self::rschnorr::RistrittoSignatureError;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum SignatureError {
    Unknown,
    DataConversionError(String),
    SignatureConstructionError,
}

fn make_rng() -> rand::rngs::StdRng {
    rand::rngs::StdRng::from_entropy()
}

#[derive(Debug, PartialEq, Eq, Clone, DecodeDer, EncodeDer)]
pub enum KeyKind {
    RistrettoSchnorr,
}

#[derive(Debug, PartialEq, Eq, Clone, DecodeDer, EncodeDer)]
pub struct PrivateKey {
    key: PrivateKeyHolder,
}

#[derive(Debug, PartialEq, Eq, Clone, DecodeDer, EncodeDer)]
pub struct PublicKey {
    pub_key: PublicKeyHolder,
}

#[derive(Debug, PartialEq, Eq, Clone, DecodeDer, EncodeDer)]
pub(crate) enum PrivateKeyHolder {
    RistrettoSchnorr(rschnorr::MLRistrettoPrivateKey),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, DecodeDer, EncodeDer)]
pub(crate) enum PublicKeyHolder {
    RistrettoSchnorr(rschnorr::MLRistrettoPublicKey),
}

impl From<RistrittoSignatureError> for SignatureError {
    fn from(e: RistrittoSignatureError) -> Self {
        match e {
            RistrittoSignatureError::ByteConversionError(s) => {
                SignatureError::DataConversionError(s)
            }
        }
    }
}

impl PrivateKey {
    pub fn new(key_kind: KeyKind) -> (PrivateKey, PublicKey) {
        let mut rng = make_rng();
        match key_kind {
            KeyKind::RistrettoSchnorr => {
                let k = rschnorr::MLRistrettoPrivateKey::new(&mut rng);
                (
                    PrivateKey {
                        key: PrivateKeyHolder::RistrettoSchnorr(k.0),
                    },
                    crate::key::PublicKey {
                        pub_key: PublicKeyHolder::RistrettoSchnorr(k.1),
                    },
                )
            }
        }
    }

    pub fn kind(&self) -> KeyKind {
        match self.key {
            PrivateKeyHolder::RistrettoSchnorr(_) => KeyKind::RistrettoSchnorr,
        }
    }

    pub(crate) fn get_internal_key(&self) -> &PrivateKeyHolder {
        &self.key
    }

    pub fn sign_message(&self, msg: &[u8]) -> Result<Signature, SignatureError> {
        let mut rng = make_rng();
        let k = match &self.key {
            PrivateKeyHolder::RistrettoSchnorr(k) => k,
        };
        let sig = k.sign_message(&mut rng, msg)?;
        Ok(Signature::RistrettoSchnorrSig(sig))
    }
}

impl PublicKey {
    pub fn from_private_key(private_key: &PrivateKey) -> Self {
        match private_key.get_internal_key() {
            PrivateKeyHolder::RistrettoSchnorr(ref k) => crate::key::PublicKey {
                pub_key: PublicKeyHolder::RistrettoSchnorr(
                    rschnorr::MLRistrettoPublicKey::from_private_key(k),
                ),
            },
        }
    }

    pub fn verify_message(&self, signature: &Signature, msg: &[u8]) -> bool {
        use crate::key::Signature::RistrettoSchnorrSig;

        let k = match &self.pub_key {
            PublicKeyHolder::RistrettoSchnorr(k) => k,
        };
        match signature {
            RistrettoSchnorrSig(s) => k.verify_message(s, msg),
        }
    }

    pub fn is_aggregable(&self) -> bool {
        match self.pub_key {
            PublicKeyHolder::RistrettoSchnorr(_) => true,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let (sk, pk) = PrivateKey::new(KeyKind::RistrettoSchnorr);
        assert_eq!(sk.kind(), KeyKind::RistrettoSchnorr);
        let msg_size = 1 + rand::random::<usize>() % 10000;
        let msg: Vec<u8> = (0..msg_size).map(|_| rand::random::<u8>()).collect();
        let sig = sk.sign_message(&msg).unwrap();
        assert!(pk.verify_message(&sig, &msg));
    }
}
