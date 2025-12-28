use crate::ws;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

struct Client {
    id: u64,
    stream: TcpStream,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static HUB: OnceLock<Mutex<Vec<Client>>> = OnceLock::new();

fn hub() -> &'static Mutex<Vec<Client>> {
    HUB.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn register(stream: TcpStream) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let writer = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };

    hub().lock().unwrap().push(Client { id, stream: writer });

    thread::spawn(move || {
        let mut reader = stream;
        loop {
            match ws::read_frame(&mut reader) {
                Ok(frame) => match frame.opcode {
                    0x8 => break,
                    0x9 => {
                        let _ = send_pong(id, &frame.payload);
                    }
                    _ => {}
                },
                Err(_) => break,
            }
        }
        remove(id);
    });
}

pub fn broadcast(message: &str) {
    let mut hub = hub().lock().unwrap();
    let mut dead = Vec::new();
    for client in hub.iter_mut() {
        if ws::write_text(&mut client.stream, message).is_err() {
            dead.push(client.id);
        }
    }
    if !dead.is_empty() {
        hub.retain(|c| !dead.contains(&c.id));
    }
}

fn send_pong(id: u64, payload: &[u8]) -> bool {
    let mut hub = hub().lock().unwrap();
    for client in hub.iter_mut() {
        if client.id == id {
            return ws::write_pong(&mut client.stream, payload).is_ok();
        }
    }
    false
}

fn remove(id: u64) {
    let mut hub = hub().lock().unwrap();
    hub.retain(|c| c.id != id);
}
