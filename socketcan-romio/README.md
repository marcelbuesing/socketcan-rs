# Socketcan-romio

This crate supports reading CAN frames from [CAN_BCM Sockets](https://www.kernel.org/doc/html/v4.16/networking/can.html?broadcast-manager-protocol-sockets-sock-dgram#broadcast-manager-protocol-sockets-sock-dgram) using async/await via the [futures-preview](https://crates.io/crates/futures-preview) and [romio](https://crates.io/crates/romio) crates.

# Example

This example shows how to filter for one specific CAN frame by can id.

```Rust
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

    const CAN_MSG_ID: u32 = 0x123;

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
```