#![feature(proc_macro, proc_macro_non_items, generators)]

extern crate futures_await as futures;
extern crate socketcan;
extern crate tokio;

use futures::prelude::*;
use socketcan::bcm::async::*;
use socketcan::FrameFlags;
use std::time;

fn main() {
    let socket = CanBCMSocket::open_nb("vcan0").unwrap();
    let ival = time::Duration::from_millis(0);
    let f = async_block! {
        #[async]
        for frame in socket
            .filter_id_incoming_frames(0x123, ival, ival, FrameFlags::empty())
            .unwrap()
            .map_err(|err| eprintln!("IO error {:?}", err)) {

            println!("Frame {:?}", frame);
        }
        Ok(())
    };
    tokio::run(f);
}
