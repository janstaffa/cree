use cree::{join_bytes, Error};
use crypto::mac::Mac;
use rand_core::{OsRng, RngCore};

use super::crypto::{ECCurve, EphemeralPair};
use super::digest::HmacSha256;
use super::{Certificate, CipherSuite, KeyExchange, TLSExtension, TLSRecord, TLSVersion};

pub struct TLSSession {
    pub is_encrypted: bool,
    pub server_random: [u8; 32],
    pub client_random: Option<Vec<u8>>,
    pub client_public_key: Option<[u8; 32]>,
    pub ephemeral_pair: EphemeralPair,
    pub master_secret: Option<Vec<u8>>,
    pub client_mac_key: Option<Vec<u8>>,
    pub server_mac_key: Option<Vec<u8>>,
    pub client_write_key: Option<Vec<u8>>,
    pub server_write_key: Option<Vec<u8>>,
    pub client_write_iv: Option<Vec<u8>>,
    pub server_write_iv: Option<Vec<u8>>,
    pub handshake_messages: Vec<TLSMessage>,
    pub incoming_encrypted_counter: u64,
    pub outgoing_encrypted_counter: u64,
}

impl TLSSession {
    pub fn new() -> TLSSession {
        let ephemeral_pair = EphemeralPair::new();

        let mut server_random = [0u8; 32];
        OsRng.fill_bytes(&mut server_random);

        TLSSession {
            is_encrypted: false,
            server_random,
            client_random: None,
            client_public_key: None,
            ephemeral_pair,
            master_secret: None,
            client_mac_key: None,
            server_mac_key: None,
            client_write_key: None,
            server_write_key: None,
            client_write_iv: None,
            server_write_iv: None,
            handshake_messages: vec![],
            incoming_encrypted_counter: 0,
            outgoing_encrypted_counter: 0,
        }
    }

    pub fn calculate_encryption_keys(&mut self) -> Result<(), Error> {
        if let (Some(client_random), Some(client_public_key)) =
            (self.client_random.clone(), self.client_public_key.clone())
        {
            let shared_key = self.ephemeral_pair.diffie_hellman(&client_public_key);

            let mut seed = b"master secret".to_vec();
            seed.append(&mut client_random.to_vec());
            seed.append(&mut self.server_random.to_vec());

            let mut mac = HmacSha256::new(shared_key.as_bytes());

            mac.input(&seed);
            let a1 = mac.result().code().to_vec();
            mac.reset();

            mac.input(&a1);
            let a2 = mac.result().code().to_vec();
            mac.reset();

            let mut input = a1;
            input.append(&mut seed.clone());
            mac.input(&input);
            let p1 = mac.result().code().to_vec();
            mac.reset();

            let mut input = a2;
            input.append(&mut seed.clone());
            mac.input(&input);
            let p2 = mac.result().code().to_vec();
            mac.reset();

            let master_secret = [&p1[..], &p2[0..16]].concat();

            let mut mac = HmacSha256::new(&master_secret);
            let mut p: Vec<u8> = vec![];
            let seed = [b"key expansion", &self.server_random[..], &client_random].concat();

            let mut prev_a = seed.clone();
            while (p.len() / 32) < 4 {
                mac.input(&prev_a);
                let a = mac.result().code().to_vec();
                mac.reset();

                let input = [&a[..], &seed[..]].concat();
                mac.input(&input);

                let mut new_p = mac.result().code().to_vec();
                p.append(&mut new_p);
                mac.reset();

                prev_a = a;
            }
            self.master_secret = Some(master_secret);
            self.client_write_key = Some(p[0..16].to_vec());
            self.server_write_key = Some(p[16..32].to_vec());
            self.client_write_iv = Some(p[32..36].to_vec());
            self.server_write_iv = Some(p[36..40].to_vec());
        } else {
            return Err(Error::new("Encryption keys cannot be calculated.", 5003));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct TLSMessage {
    pub version: TLSVersion,
    pub record: TLSRecord,
    pub content: Vec<u8>,
}

impl TLSMessage {
    pub fn new(record: TLSRecord, version: TLSVersion, content: Vec<u8>) -> TLSMessage {
        TLSMessage {
            record,
            content,
            version,
        }
    }

    pub fn get_raw(&self) -> Vec<u8> {
        let TLSMessage {
            version,
            record,
            content,
        } = &self;
        let mut raw = vec![record.get_value()];
        raw.extend(version.get_value());
        raw.extend(u16::to_be_bytes(content.len() as u16));
        raw.extend(content);

        raw
    }
}

#[derive(Debug, Clone)]
pub enum HandshakeMessage {
    // Client messages
    ClientHello {
        version: TLSVersion,
        random: Vec<u8>,
        session_id: Option<Vec<u8>>,
        cipher_suites: Vec<u16>,
        extensions: Vec<TLSExtension>,
    },
    ClientKeyExchange {
        public_key: Vec<u8>,
    },

    // Server messages
    ServerHello {
        version: TLSVersion,
        random: Vec<u8>,
        session_id: Option<Vec<u8>>,
        cipher_suite: CipherSuite,
        extensions: Vec<TLSExtension>,
    },
    ServerCertificate {
        certificates: Vec<Certificate>,
    },
    ServerKeyExchange {
        key_exchange: KeyExchange,
    },
    ServerHelloDone,

    // Shared messages
    HandshakeFinished {
        verify_data: Vec<u8>,
    },
}

impl HandshakeMessage {
    pub fn parse(data: &Vec<u8>) -> Result<HandshakeMessage, Error> {
        let message_body = &data[1..];
        match data[0] {
            // CLIENT HELLO
            1 => {
                let session_id_length = message_body[37];
                let session_id = if session_id_length > 0 {
                    Some(
                        message_body[38..(38 + session_id_length as usize)]
                        .to_vec()
                           //  .iter()
                           //  .map(|x| x.to_string())
                           //  .collect::<String>(),
                    )
                } else {
                    None
                };

                let mut cursor = 37 + session_id_length as usize;
                let cipher_suites_length = join_bytes(&message_body[(cursor + 1)..=(cursor + 2)])?;

                let mut cipher_suites: Vec<u16> = Vec::new();

                if cipher_suites_length % 2 != 0 {
                    return Err(Error::new("Invalid ClientHello message", 5001));
                }

                cursor += 2;
                for (idx, suite) in message_body
                    [(cursor + 1)..(cursor + 1 + cipher_suites_length as usize)]
                    .iter()
                    .step_by(2)
                    .enumerate()
                {
                    if let Some(next_suite) = message_body.get((cursor + 1 + idx * 2) + 1) {
                        let joined = join_bytes(&[*suite, *next_suite])?;
                        cipher_suites.push(joined as u16);
                    }
                }
                cursor += 1 + cipher_suites_length as usize;

                // skipping compression methods - not supported in TLS 1.3
                cursor += 2;

                let extensions_length = join_bytes(&data[(cursor + 1)..=(cursor + 2)])? as usize;

                let mut extensions: Vec<TLSExtension> = Vec::new();
                let mut i = 0usize;
                while i < extensions_length {
                    let c = cursor + 3 + i;
                    let ext_id = join_bytes(&data[c..=(c + 1)])? as u16;
                    let ext_length = join_bytes(&data[(c + 2)..=(c + 3)])?;
                    extensions.push(TLSExtension::new(
                        ext_id,
                        // ext_length as u16,
                        data[(c + 4)..(c + 4 + ext_length as usize)].to_vec(),
                    ));

                    i += 4 + ext_length as usize;
                }

                Ok(HandshakeMessage::ClientHello {
                    //   length: join_bytes(&message_body[0..=2])? as usize,
                    version: TLSVersion::from(&message_body[3..=4])?,
                    random: message_body[5..=36].to_vec(),
                    session_id,
                    cipher_suites,
                    extensions,
                })
            }
            // CLIENT KEY EXCHANGE
            16 => {
                let public_key_length = message_body[3] as usize;

                Ok(HandshakeMessage::ClientKeyExchange {
                    //   length: join_bytes(&message_body[0..=2])? as usize,
                    public_key: message_body[4..(4 + public_key_length)].to_vec(),
                })
            }

            // FINISHED
            20 => Ok(HandshakeMessage::HandshakeFinished {
                //  length: join_bytes(&message_body[0..=2])? as usize,
                verify_data: message_body[3..].to_vec(),
            }),
            _ => Err(Error::new("Unknown message type.", 5002)),
        }
    }

    pub fn get_raw(&self) -> Result<Vec<u8>, Error> {
        let mut response = vec![];
        match &self {
            &Self::ServerHello {
                version,
                random,
                session_id,
                cipher_suite,
                extensions,
            } => {
                // ServerHello type value = 0x02
                response.push(0x02);

                let session_id_length = session_id.as_ref().unwrap_or(&vec![]).len();
                if session_id_length > u8::MAX as usize {
                    return Err(Error::new(
                        "Provided session ID is too long. (max 255 bytes)",
                        5004,
                    ));
                }
                let extensions_length = extensions
                    .iter()
                    .fold(0, |acc, ext| acc + 4 + ext.content.len());

                /*
                  2 = server version
                  1 = session_id length field
                  2 = cipher suite
                  1 = compression method
                  2 = extensions length field
                */
                let length =
                    2 + random.len() + 1 + session_id_length + 2 + 1 + 2 + extensions_length;

                // Message length (3 bytes)
                response.extend(&(length as u32).to_be_bytes()[1..]);

                // Server version
                response.extend(&version.get_value());

                // Server random
                response.extend(random);

                // Session ID
                response.push(session_id_length as u8);
                if let Some(session_id) = session_id {
                    response.extend(session_id);
                }

                // Selected cipher suite
                response.extend(&cipher_suite.bytes());

                // Compression - null
                response.push(0x00);

                // Extension list
                response.extend(&(extensions_length as u16).to_be_bytes());
                for extension in extensions {
                    let mut extension_bytes = extension.id.to_be_bytes().to_vec();
                    extension_bytes.extend(&(extension.content.len() as u16).to_be_bytes());
                    extension_bytes.extend(&extension.content);
                    response.extend(&extension_bytes);
                }
            }
            &Self::ServerCertificate { certificates } => {
                // Certificate type = 0x0b
                response.push(0x0b);

                /*
                  3 = certificates list length field
                */
                let length = 3 + certificates
                    .iter()
                    .fold(0, |acc, cert| acc + 3 + cert.raw.len());

                // Full message length
                response.extend(&(length as u32).to_be_bytes()[1..]);

                // Certificates list length
                response.extend(&((length - 3) as u32).to_be_bytes()[1..]);

                // Append each cerificate
                for certificate in certificates {
                    let mut certificate_bytes =
                        (certificate.raw.len() as u32).to_be_bytes()[1..].to_vec();
                    certificate_bytes.extend(&certificate.raw);
                    response.extend(&certificate_bytes);
                }
            }

            &Self::ServerKeyExchange { key_exchange } => {
                // ServerKeyExchange type = 0x0c
                response.push(0x0c);
                match key_exchange {
                    KeyExchange::ECDHE { curve, public_key } => {
                        /*
                         3 = curve info
                         1 = public key length field
                         2 = signature type field
                         2 = signature length field
                        */

                        let length = 3 + 1 + public_key.len() + 2 + 2;

                        // Full message length
                        response.extend(&(length as u32).to_be_bytes()[1..]);

                        // Named curve byte
                        response.push(0x03);
                        match curve {
                            &ECCurve::x25519 => {
                                // assigned x25519 value
                                response.extend(&[0x00, 0x1d]);
                            }
                        }
                        if public_key.len() > u8::MAX as usize {
                            return Err(Error::new(
                                "Invalid public key. (max length 255 bytes)",
                                5004,
                            ));
                        }

                        // Public key with its length
                        response.push(public_key.len() as u8);
                        response.extend(public_key);

                        // if let Some(signature) = signature {
                        //     // Two byte long signature type field
                        //     match signature.signature_type {
                        //         Signature::RSA_SHA256 => {
                        //             // assigned RSA with SHA256 signature value
                        //             response.extend(&[0x04, 0x01]);
                        //         }
                        //     }

                        //     // Two byte length of the signature
                        //     response.extend((signature.data.len() as u16).to_be_bytes());

                        //     // The signature itself
                        //     response.extend(&signature.data);
                        // }
                    }
                }
            }
            &Self::ServerHelloDone => {
                // ServerHelloDone type = 0x0e
                response.push(0x0e);

                // 0 bytes of message
                response.extend(&[0x00, 0x00, 0x00]);
            }
            &Self::HandshakeFinished { verify_data } => {
                // HandshakeFinished type = 0x14
                response.push(0x14);

                // 3 bytes message length
                response.extend(&(verify_data.len() as u32).to_be_bytes()[1..]);

                response.extend(verify_data);
            }
            _ => {}
        };
        Ok(response)
    }
}

pub fn parse_tls_messages(data: &[u8]) -> Result<Vec<TLSMessage>, Error> {
    if data.len() == 0 {
        return Err(Error::new("Invalid message.", 5001));
    }

    let mut messages: Vec<TLSMessage> = vec![];
    let mut cursor = 0;
    while cursor < data.len() - 1 {
        let record = match data[cursor] {
            0x14 => TLSRecord::ChangeCipherSpec,
            0x15 => TLSRecord::Alert,
            0x16 => TLSRecord::Handshake,
            0x17 => TLSRecord::Application,
            0x18 => TLSRecord::Heartbeat,
            _ => return Err(Error::new("Invalid message", 5001)),
        };
        let length = join_bytes(&data[(cursor + 3)..=(cursor + 4)])? as usize;

        let version = match data[cursor + 1..=cursor + 2] {
            [0x03, 0x01] => TLSVersion::TLS1_0,
            [0x03, 0x02] => TLSVersion::TLS1_1,
            [0x03, 0x03] => TLSVersion::TLS1_2,
            _ => return Err(Error::new("Unsupported TLS version.", 5005)),
        };
        let message = TLSMessage {
            record,
            version,
            content: data[(cursor + 5)..(cursor + 5 + length)].to_vec(),
        };

        // message body
        cursor += length;

        // meta (record, verison, length)
        cursor += 5;

        messages.push(message);
    }
    Ok(messages)
}
