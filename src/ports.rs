//! Port allocation helpers for dev/prod environments.
use std::collections::HashSet;
use std::net::TcpListener;

use crate::db::{self, Value};

pub fn find_free_port_pair(
    dev_start: u16,
    dev_end: u16,
    prod_start: u16,
    prod_end: u16,
) -> Option<(u16, u16)> {
    if dev_start > dev_end || prod_start > prod_end {
        return None;
    }
    let delta = prod_start as i32 - dev_start as i32;
    let used = assigned_ports();

    for dev in dev_start..=dev_end {
        let prod = (dev as i32 + delta) as i32;
        if prod < prod_start as i32 || prod > prod_end as i32 {
            continue;
        }
        let dev_u16 = dev;
        let prod_u16 = prod as u16;
        if used.contains(&dev_u16) || used.contains(&prod_u16) {
            continue;
        }
        if is_port_free(dev_u16) && is_port_free(prod_u16) {
            return Some((dev_u16, prod_u16));
        }
    }

    None
}

pub fn ip_from_port(base: &str, port: u16) -> Option<String> {
    let base_port = (port / 100) * 100;
    let offset = port.saturating_sub(base_port);
    let ip_octet = offset.saturating_add(1);
    if offset == 0 || ip_octet > 254 {
        return None;
    }
    Some(format!("{}{}", base, ip_octet))
}

fn assigned_ports() -> HashSet<u16> {
    let mut used = HashSet::new();
    let docs = db::get().find_all("_ports");
    for doc in docs {
        if let Some(Value::Int(port)) = doc.get("dev_port") {
            if *port >= 0 && *port <= u16::MAX as i64 {
                used.insert(*port as u16);
            }
        }
        if let Some(Value::Int(port)) = doc.get("prod_port") {
            if *port >= 0 && *port <= u16::MAX as i64 {
                used.insert(*port as u16);
            }
        }
    }
    used
}

fn is_port_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}
