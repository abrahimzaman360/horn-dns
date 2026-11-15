use std::{collections::HashMap, fmt::Debug};

#[allow(unused)]
struct IPv4(u8, u8, u8, u8); // IPv4

struct DNServer {
    record: HashMap<String, IPv4>,
}

#[allow(unused)]
#[derive(Debug)]
struct ErrorType {
    status: u16,
    msg: String,
}

#[derive(Debug)]
struct DNSRecord {
    domain: String,
    ip: IPv4,
}

impl Debug for IPv4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0, self.1, self.2, self.3)
    }
}

impl DNServer {
    fn new() -> Self {
        Self {
            record: HashMap::new(),
        }
    }

    fn get(&self, s: &str) -> Option<&IPv4> {
        match self.record.get(s) {
            Some(v) => Some(v),
            _ => None,
        }
    }

    fn set(&mut self, r: DNSRecord) -> Result<(), ErrorType> {
        if self.record.contains_key(&r.domain) {
            return Err(ErrorType {
                status: 409,
                msg: "Domain already exists!".to_string(),
            });
        }

        self.record.insert(r.domain, r.ip);
        Ok(())
    }
}

fn main() {
    let mut dns_server = DNServer::new();

    dns_server
        .set(DNSRecord {
            domain: "google.com".to_string(),
            ip: IPv4(192, 168, 1, 2),
        })
        .unwrap();

    let res = dns_server.get("google.com").unwrap();
    println!("DNS Responded: {:?}", res);
}
