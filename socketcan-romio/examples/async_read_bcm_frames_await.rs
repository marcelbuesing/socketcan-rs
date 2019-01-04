#![feature(async_await, await_macro, futures_api)]

use std::io;

use futures::executor;
use futures::StreamExt;
use socketcan_romio::bcm::*;
use socketcan::FrameFlags;
use std::time;

fn main() -> io::Result<()> {
    let socket = CanBCMSocket::open_nb("vcan0").unwrap();
    let ival = time::Duration::from_millis(0);

    executor::block_on(async {

        let mut incoming = socket
            .filter_id_incoming_frames(0x123, ival, ival, FrameFlags::empty())
            .unwrap();

         while let Some(frame) = await!(incoming.next()) {
             println!("Frame {:?}", frame);
         }

         Ok(())
    })
}
