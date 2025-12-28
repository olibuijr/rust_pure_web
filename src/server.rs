use std::net::TcpListener;
use crate::handler;

pub fn run(addr: &str) {
    let listener = TcpListener::bind(addr).unwrap();
    for stream in listener.incoming().flatten() {
        handler::handle(stream);
    }
}
