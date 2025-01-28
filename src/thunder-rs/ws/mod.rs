pub mod error;
pub mod frame;
pub mod handshake;
pub mod opcode;
pub mod server;
pub mod socket {
    pub mod socket;
    pub mod write;
    pub mod read;
    pub mod room;
}