
extern crate futures;
extern crate socketcan;
extern crate socketcan_tokio;
extern crate tokio;

use futures::stream::Stream;
use socketcan::{CanFrame, CanSocket};
use socketcan_tokio::bcm::*;
use tokio::runtime::Runtime;
use std::time;

fn send_frame() {
    let cs = CanSocket::open("vcan0").unwrap();
    let frame = CanFrame::new(0x123, &[], false, false).unwrap();
    cs.write_frame(&frame).unwrap();
}

#[test]
fn vcan0_bcm_filter_id_incoming_frames() {
    let cbs = CanBCMSocket::open_nb("vcan0").unwrap();
    let ival = time::Duration::from_millis(1);
    cbs.filter_id(0x123, ival, ival, Default::default()).unwrap();

    let next_frame =
        cbs.incoming_frames()
           .map_err(|_| ())
           .into_future();

    send_frame();

    let (msg_head, _) =
        Runtime::new()
         .unwrap()
         .block_on(next_frame)
         .map_err(|_| ())
         .expect("Failed");

    assert!(msg_head.unwrap().id() == 0x123);
}


#[test]
fn vcan0_bcm_filter_delete() {
    let cbs = CanBCMSocket::open_nb("vcan0").unwrap();
    let ival = time::Duration::from_millis(1);
    cbs.filter_id(0x123, ival, ival, Default::default()).unwrap();

    cbs.filter_delete(0x123).unwrap();
}

#[test]
fn vcan0_bcm_filter_delete_err() {
    let cbs = CanBCMSocket::open_nb("vcan0").unwrap();
    assert!(cbs.filter_delete(0x124).is_err())
}