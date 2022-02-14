use crate::core::http::codes::HTTPStatus;
use crate::core::http::protocol::{Response, Method};
use crate::core::service::CreeService;
use crate::core::tcp::TCPConnection;
use crate::core::tls::{TLSRecord, TLSVersion, CipherSuite, TLSExtension, Certificate, KeyExchange};
use crate::core::tls::crypto::{ECCurve, EncryptedMessage};
use crate::core::tls::digest::{DigestAlgorithm, HmacSha256};
use crate::core::tls::protocol::{
    parse_tls_messages, HandshakeMessage,
    TLSMessage, TLSSession
};
use crate::core::tls::signature::{RSASignature, Signature};
use bytes::Buf;
use clap::{App, SubCommand};
use clap::Arg;
use cree::{CreeOptions, Error};
use crypto::digest::Digest;
use crypto::mac::Mac;

use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

mod core;
use crate::core::connection::HTTPConnection;
use std::{fs, vec};

mod extensions;


use crypto::sha2::Sha256;


use base64;

#[tokio::main]
async fn main() {
    // read cli arguments
    let matches = App::new("Cree")
        .version("0.1.0")
        .subcommand(
            SubCommand::with_name("start")
                .arg(
                    Arg::with_name("path")
                        .short('p')
                        .long("path")
                        .takes_value(true)
                        .required(false),
                )
                .arg(
                    Arg::with_name("port")
                        .long("port")
                        .takes_value(true)
                        .required(false),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("start") {
        let port = matches.value_of("port");
        let path = matches.value_of("path");

        let mut options = CreeOptions::get_default();
        let conf_file = fs::read(PathBuf::from("cree.toml"));
        if let Ok(f) = conf_file {
            options = match toml::from_slice::<CreeOptions>(&f)
                .or(Err(Error::new("Failed to read configuration file.", 1005)))
            {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("{}", e.msg);
                    return;
                }
            }
        } else {
            eprintln!("No cree conf file found.");
        }

        // check if port was passed in cli, if not check the config file or use a default(80)
        let port = if let Some(p) = port {
            p.parse().unwrap_or(80)
        } else {
            if let Some(p) = options.port {
                p
            } else {
                80
            }
        };

        if let Some(path) = path {
            options.root_directory = Some(PathBuf::from(&path));
        }

        if let Some(path) = &options.root_directory {
            if !path.exists() {
                return eprintln!(
                    "server error: Path {} is not a valid path",
                    &path.to_str().unwrap()
                );
            }

            // the service holds information about the server configuration and is cloned into every connections thread

            let service = match CreeService::new(path, &options) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{}", e.msg);
                    return;
                }
            };
            let service = Arc::new(service);
            let mut threads = vec![];

            threads.push(tokio::spawn(async move {
                let service = service.clone();
                // socket address to listen on
                let addr = SocketAddr::from(([0, 0, 0, 0], port));

                let listener = TcpListener::bind(addr).await.unwrap();
                println!("Listening on {}", addr);

                // listen for new connections
                while let Ok((socket, _)) = listener.accept().await {
                    // receieve raw message from TCPConnection
                    let service = service.clone();

                    // spawn a new thread and move in the service
                    tokio::spawn(async move {
                        // construct an HTTPConnection which wraps the connection streams and reveals higher level API
                        let mut tcp_connection = HTTPConnection::new(socket).unwrap();

                        // listen for individual requests on the connection (this makes persistent connections work)
                        // the listen_for_requests() method will halt until a request is received
                        while let Ok(req) = tcp_connection.listen_for_requests().await {
                            if let Method::Unknown = &req.method {
                                let mut res = Response::new(req);
                                res.set_header("Allow", "GET,HEAD,POST");
                                res.set_status(HTTPStatus::MethodNotAllowed);
                                tcp_connection.write_response(res, true).await.unwrap();
                                continue;
                            }

                            // println!("request: {}", req.uri);

                            // construct a response according to the request
                            let res = service.create_response(req).await.unwrap();

                            let use_compression = if let Some(uc) = options.use_compression {
                                uc
                            } else {
                                false
                            };

                            // send the response to the client
                            tcp_connection
                                .write_response(res, use_compression)
                                .await
                                .unwrap();
                        }

                        // the above loop will exit if an error is returned from listen_for_requests(), which means the connection has to be closed
                        tcp_connection.close().await.unwrap();
                        //   println!("connection closed: {:?}", remote_addr);
                    });
                }
            }));

            let service = match CreeService::new(path, &options) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{}", e.msg);
                    return;
                }
            };
            let service = Arc::new(service);
            threads.push(tokio::spawn(async move {
               let service = service.clone();

               // socket address to listen on
               let addr = SocketAddr::from(([0, 0, 0, 0], 443));

               let listener = TcpListener::bind(addr).await.unwrap();
               println!("Listening on {}", addr);

               // listen for new connections
                while let Ok((socket, _)) = listener.accept().await {
                    tokio::spawn(async move {
                   let tcp_connection = TCPConnection::new(socket);
                   if let Ok(mut tcp_connection) = tcp_connection {
                       let mut tls_session = TLSSession::new();

                       while let Ok(tcp_message) = tcp_connection.read_message().await {
                          // TLS 1.2 implementation
                          let messages = parse_tls_messages(&tcp_message.content).unwrap();

                          for tls_message in messages {
                              // println!("message: {:?}", tls_message);
                             let real_message = if tls_session.is_encrypted {
                                let decrypted_content = if let (Some(client_write_key), Some(client_write_iv)) =  (&tls_session.client_write_key, &tls_session.client_write_iv) {
                                   let encryption_iv = [&client_write_iv[..], &tls_message.content[0..8]].concat();

                                   let data = &tls_message.content[8..];

                                   let mut aad = tls_session.incoming_encrypted_counter.to_be_bytes().to_vec();
                                   aad.push(tls_message.record.get_value());
                                   aad.extend(tls_message.version.get_value());
                                   aad.extend(((data.len() - 16) as u16).to_be_bytes());

                                   EncryptedMessage::decrypt(data, &encryption_iv, client_write_key, &aad).unwrap()
                              } else {
                                    panic!("Invalid handshake order.");
                                 };

                                 tls_session.incoming_encrypted_counter += 1;

                              let mut new_message = tls_message;
                              new_message.content = decrypted_content;
                              new_message
                           } else { tls_message };
                           match real_message.record {
                              TLSRecord::Handshake => {
                                 let handshake_message = HandshakeMessage::parse(&real_message.content).unwrap();


                                 match handshake_message {
                                    HandshakeMessage::ClientHello { random: client_random, .. } => {
                                          tls_session.handshake_messages.push(real_message);

                                          tls_session.client_random = Some(client_random.clone());

                                          let mut bulk_write: Vec<u8> = vec![];

                                          //
                                          // Server Hello
                                          //

                                          let server_hello = HandshakeMessage::ServerHello {
                                             version: TLSVersion::TLS1_2,
                                             cipher_suite: CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
                                             random: tls_session.server_random.to_vec(),
                                             session_id: None,
                                             extensions: vec![TLSExtension::new(65281, vec![0])]
                                          };

                                          let server_hello_handshake = TLSMessage::new(TLSRecord::Handshake, TLSVersion::TLS1_2, server_hello.get_raw().unwrap());
                                          bulk_write.extend(&server_hello_handshake.get_raw());
                                          tls_session.handshake_messages.push(server_hello_handshake);


                                          //
                                          // Server Certificate
                                          //

                                          let mut certificates = vec![];
                                          let certificate = fs::read_to_string(
                                             "C:\\Users\\admin\\Desktop\\Projects\\RUST\\cree\\certs\\cert.crt",
                                          ).unwrap();
                                          let certificate = base64::decode(certificate).unwrap();
                                          certificates.push(Certificate {raw: certificate});

                                          let server_certificate = HandshakeMessage::ServerCertificate {
                                             certificates
                                          };
                                          let server_certificate_handshake = TLSMessage::new(TLSRecord::Handshake, TLSVersion::TLS1_2, server_certificate.get_raw().unwrap());
                                          bulk_write.extend(&server_certificate_handshake.get_raw());
                                          tls_session.handshake_messages.push(server_certificate_handshake);

                                          //
                                          // Server Key Exchange
                                          //

                                          let server_key_exchange = HandshakeMessage::ServerKeyExchange {
                                             key_exchange: KeyExchange::ECDHE{curve: ECCurve::x25519,
                                                public_key: tls_session.ephemeral_pair.public_key().as_bytes().to_vec()
                                             }};

                                          let mut key_exchange_body = server_key_exchange.get_raw().unwrap()[4..].to_vec();
                                          // signature
                                          let mut to_sign: Vec<u8> = vec![];
                                          to_sign.extend(&client_random);
                                          to_sign.extend(&tls_session.server_random);
                                          to_sign.extend(&key_exchange_body);

                                          // signature
                                          let private_key_der = fs::read(
                                             "C:\\Users\\admin\\Desktop\\Projects\\RUST\\cree\\certs\\private.key",
                                          ).unwrap();

                                          let signer = RSASignature::new(private_key_der);
                                          let signature = signer.sign(DigestAlgorithm::SHA256, &to_sign).unwrap();

                                          let signature_id = match signature.signature_scheme {
                                             Signature::RSA_SHA256 => {
                                                // assigned RSA with SHA256 signature value
                                                [0x04, 0x01]}
                                          };
                                          key_exchange_body.extend(&signature_id);
                                          key_exchange_body.extend(u16::to_be_bytes(signature.data.len() as u16));
                                          key_exchange_body.extend(&signature.data);

                                          let mut key_exchange_message = vec![0x0c];
                                          key_exchange_message.extend(&u32::to_be_bytes(key_exchange_body.len() as u32)[1..]);
                                          key_exchange_message.extend(&key_exchange_body);

                                          let server_key_exchange_handshake = TLSMessage::new(TLSRecord::Handshake, TLSVersion::TLS1_2, key_exchange_message);
                                          bulk_write.extend(&server_key_exchange_handshake.get_raw());
                                          tls_session.handshake_messages.push(server_key_exchange_handshake);

                                          //
                                          // Server Hello Done
                                          //
                                          let server_hello_done = HandshakeMessage::ServerHelloDone;

                                          let server_hello_done_handshake = TLSMessage::new(TLSRecord::Handshake, TLSVersion::TLS1_2, server_hello_done.get_raw().unwrap());
                                          bulk_write.extend(&server_hello_done_handshake.get_raw());
                                          tls_session.handshake_messages.push(server_hello_done_handshake);


                                          tcp_connection.write(&bulk_write).await.unwrap();

                                       },
                                       HandshakeMessage::ClientKeyExchange { public_key, .. } => {
                                          tls_session.handshake_messages.push(real_message);
                                          let mut buf = [0u8; 32];

                                          let mut reader = public_key.reader();
                                          reader.read(&mut buf).unwrap();

                                          tls_session.client_public_key = Some(buf);
                                       },

                                       HandshakeMessage::HandshakeFinished {verify_data, ..} => {
                                          let mut to_hash: Vec<u8> = vec![];
                                          for message in &tls_session.handshake_messages {
                                             to_hash.extend(&message.content);
                                          }


                                          let mut sha256_encryptor = Sha256::new();
                                          sha256_encryptor.input(&to_hash);

                                          let mut hash = [0u8; 32];
                                          sha256_encryptor.result(&mut hash);


                                          let mut seed = b"client finished".to_vec();
                                          seed.extend(&hash);

                                          if let Some(master_secret) = &tls_session.master_secret {
                                             let mut mac = HmacSha256::new(master_secret);
                                             mac.input(&seed);
                                             let a1 = mac.result().code().to_vec();
                                             mac.reset();

                                             mac.input(&[a1, seed].concat());
                                             let p1 = mac.result().code().to_vec();

                                             let verify_data_check = &p1[..12];

                                             assert_eq!(verify_data, verify_data_check);
                                          }

                                          tls_session.handshake_messages.push(real_message);

                                          let mut bulk_write = vec![];

                                          // Server Change Cipher Spec 
                                          let server_change_cipher_spec = vec![0x14, 0x03, 0x03, 0x00, 0x01, 0x01];
                                          bulk_write.extend(server_change_cipher_spec);




                                          if let (Some(master_secret), Some(server_write_iv), Some(server_write_key)) = (&tls_session.master_secret, &tls_session.server_write_iv, &tls_session.server_write_key) {
                                             // Server Handshake Finished
                                             let mut to_hash: Vec<u8> = vec![];
                                             for message in &tls_session.handshake_messages {
                                                to_hash.extend(&message.content);
                                             }

                                             let mut sha256_encryptor = Sha256::new();
                                             sha256_encryptor.input(&to_hash);

                                             let mut hash = [0u8; 32];
                                             sha256_encryptor.result(&mut hash);

                                             let mut seed = b"server finished".to_vec();
                                             seed.extend(&hash);
                                             let mut mac = HmacSha256::new(master_secret);
                                             mac.input(&seed);
                                             let a1 = mac.result().code().to_vec();
                                             mac.reset();

                                             mac.input(&[a1, seed].concat());
                                             let p1 = mac.result().code().to_vec();

                                             let verify_data = p1[..12].to_vec();

                                             let server_handshake_finished = HandshakeMessage::HandshakeFinished {
                                                verify_data
                                             };

                                             let data = server_handshake_finished.get_raw().unwrap();

                                             let sequence_number = tls_session.outgoing_encrypted_counter.to_be_bytes();
                                             // constructing IV - SERVER_WRITE_IV + sequence number 
                                             let mut iv = server_write_iv.clone();
                                             iv.extend(&sequence_number);


                                             // constructing AAD - sequence number + record header

                                             let mut aad = sequence_number.to_vec();
                                             aad.push(0x16);
                                             aad.extend(&[0x03, 0x03]);
                                             aad.extend((data.len() as u16).to_be_bytes());


                                             let encrypted = EncryptedMessage::encrypt(&data, &iv, server_write_key, &aad);
                                             let mut data = sequence_number.to_vec();
                                             data.extend(encrypted);

                                             let server_handshake_finished_record = TLSMessage::new(TLSRecord::Handshake, TLSVersion::TLS1_2, data);
                                             bulk_write.extend(server_handshake_finished_record.get_raw());

                                             tcp_connection.write(&bulk_write).await.unwrap();

                                             tls_session.outgoing_encrypted_counter += 1;
                                          }
                                       },

                                       _ => {
                                          // Err(Error::new("Invalid TLS client message.", 5001))
                                       }
                                 };


                              }
                              TLSRecord::ChangeCipherSpec => {
                                 tls_session.calculate_encryption_keys().unwrap();
                                 tls_session.is_encrypted = true;



                                 println!("received CHANGE_CIPHER_SPEC");
                              }
                              TLSRecord::Alert => {
                                 println!("received ALERT: {:?}", real_message.content);
                              }
                              TLSRecord::Application => {
                                 println!("received APPLICATION");
                                 println!("{:?}", String::from_utf8_lossy(&real_message.content));


                                 if let (Some(server_write_iv), Some(server_write_key)) = (&tls_session.server_write_iv, &tls_session.server_write_key) {
                                    
                                    let sequence_number = tls_session.outgoing_encrypted_counter.to_be_bytes();
                                    // constructing IV - SERVER_WRITE_IV + sequence number 
                                    let mut iv = server_write_iv.clone();
                                    iv.extend(&sequence_number);
                                    
                                    
                                    let html = "<h1>Hello from HTTPS server!</h1>";
                                    let http = format!("HTTP/1.1 200 OK\nContent-type: text/html\nContent-Length: {}\n\n{}", html.len(), html);
                                    let data = http.as_bytes();

                                    // constructing AAD - sequence number + record header
                                    let mut aad = sequence_number.to_vec();
                                    aad.push(0x17);
                                    aad.extend(&[0x03, 0x03]);
                                    aad.extend((data.len() as u16).to_be_bytes());
                                 
                                 
                                    let encrypted = EncryptedMessage::encrypt(&data, &iv, server_write_key, &aad);
                                    let mut data = sequence_number.to_vec();
                                    data.extend(encrypted);

                                    let application_message = TLSMessage::new(TLSRecord::Application, TLSVersion::TLS1_2, data);

                                    println!("application_message: {:?}", application_message.get_raw());

                                    tcp_connection.write(&application_message.get_raw()).await.unwrap();
                                    tls_session.outgoing_encrypted_counter += 1;

                                 }
                              }
                              TLSRecord::Heartbeat => {
                                 println!("received HEARTBEAT");
                              }
                           }
                        }
                     }
                  }
               });
               }
            }));
            futures::future::join_all(threads).await;
        } else {
            eprintln!("No root directory specified.");
            return;
        }
    }
}

// TODO: Add custom error page option to cree.toml.
// TODO: REDIRECT_STATUS in php should be dynamic (200, 400, 500,...)
// TODO: add logging option to CreeServer
// TODO: HTTPS
// TODO: add more options to 'cree.toml'
// TODO: change all Vec<u8> to Bytes
// TODO: add partial content streaming.
// TODO: support pipelining
