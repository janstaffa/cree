use std::env;
extern crate clap;
use clap::{App, Arg, SubCommand};
use std::path::PathBuf;

enum Action {
    start,
    stop,
}
struct Options {
    action: Action,
    path: PathBuf,
    port: u16
}

fn main() {
    let matches = App::new("Cree")
        .version("0.1.0")
        .subcommand(
           SubCommand::with_name("start")
           .arg(Arg::new("path")
            .short('p')
            .long("path")
            .takes_value(true)
            .about("Path to the folder you want to serve")
            .required(true)
           )  
           .arg(Arg::new("port")
            .long("port")
            .takes_value(true)
            .about("Port to serve on (default: 80)")
            .default_missing_value("80")
            .required(false)
           )
        )
        .get_matches();
        if let Some(matches) = matches.subcommand_matches("start"){
           println!("path: {}", matches.value_of("path").unwrap());
           if let Some(port) = matches.value_of("port"){

              println!("port: {}", port);
            }

        }
}