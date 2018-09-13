use futures::stream::Stream;
use socketcan::bcm_tokio::*;
use socketcan::FrameFlags;
use std::time;

fn main() {
    let socket = CanBCMSocket::open_nb("vcan0").unwrap();
    let ival = time::Duration::from_millis(0);
    let f = socket
        .filter_id_incoming_frames(0x123, ival, ival, FrameFlags::empty())
        .unwrap()
        .map_err(|err| eprintln!("IO error {:?}", err))
        .for_each(|frame| {
            println!("Frame {:?}", frame);
            Ok(())
        });
    tokio::run(f);
}
