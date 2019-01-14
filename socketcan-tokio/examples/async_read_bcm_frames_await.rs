#![feature(proc_macro, proc_macro_hygiene, generators)]

extern crate futures_await as futures;

//use futures_await::async_block;
use futures::prelude::*;
use socketcan_tokio::bcm::*;
use socketcan::FrameFlags;
use std::time;

fn main() {
    let socket = CanBCMSocket::open_nb("vcan0").unwrap();
    let ival = time::Duration::from_millis(0);
    let f = async_block! {
        #[async]
        for frame in socket
            .filter_id_incoming_frames(0x123.into(), ival, ival)
            .unwrap()
            .map_err(|err| eprintln!("IO error {:?}", err)) {

            println!("Frame {:?}", frame);
        }
        Ok(())
    };
    tokio::run(f);
}
