use crate::{CanFrame, CanMessageId, CanSocket, ConstructionError};
use std::convert::TryFrom;

#[test]
fn test_nonexistant_device() {
    assert!(CanSocket::open("invalid").is_err());
}

#[test]
fn can_message_id_from_sff_u16() {
    assert_eq!(CanMessageId::SFF(255u16), CanMessageId::try_from(255u16).unwrap());
}

#[test]
fn can_message_id_from_sff_u32() {
    assert_eq!(CanMessageId::SFF(255u16), CanMessageId::try_from(255u32).unwrap());
}

#[test]
fn can_message_id_from_eff_u16() {
    assert_eq!(CanMessageId::EFF(2049), CanMessageId::try_from(2049u16).unwrap());
}

#[test]
fn can_message_id_from_eff_u32() {
    assert_eq!(CanMessageId::EFF(2049), CanMessageId::try_from(2049u32).unwrap());
}

#[test]
fn eff_bit_must_be_set_in_eff_frames() {
    let frame = CanFrame::new(CanMessageId::EFF(2049), &[0x0], false, false).unwrap();
    assert!(frame.is_extended());
}

#[test]
fn eff_bit_must_not_be_set_in_sff_frames() {
    let frame = CanFrame::new(16u16.into(), &[0x0], false, false).unwrap();
    assert!(!frame.is_extended());
}

#[test]
fn can_message_id_too_large() {
    match CanMessageId::try_from(2u32.pow(29) + 1) {
        Err(ConstructionError::IDTooLarge) => (),
        x => panic!("Must not be {:?}", x),
    }
}

#[cfg(feature = "vcan_tests")]
mod vcan_tests {
    use {CanFrame, CanInterface, CanSocket, ERR_MASK_ALL, ERR_MASK_NONE};
    use std::time;
    use ShouldRetry;

    #[test]
    fn vcan0_timeout() {
        let cs = CanSocket::open("vcan0").unwrap();
        cs.set_read_timeout(time::Duration::from_millis(100))
            .unwrap();
        assert!(cs.read_frame().should_retry());
    }

    #[test]
    fn vcan0_set_error_mask() {
        let cs = CanSocket::open("vcan0").unwrap();
        cs.set_error_mask(ERR_MASK_ALL).unwrap();
        cs.set_error_mask(ERR_MASK_NONE).unwrap();
    }

    #[test]
    fn vcan0_enable_own_loopback() {
        let cs = CanSocket::open("vcan0").unwrap();
        cs.set_loopback(true).unwrap();
        cs.set_recv_own_msgs(true).unwrap();

        let frame = CanFrame::new(0x123, &[], true, false).unwrap();

        cs.write_frame(&frame).unwrap();

        cs.read_frame().unwrap();
    }

    #[test]
    fn vcan0_set_down() {
        let can_if = CanInterface::open("vcan0").unwrap();
        can_if.bring_down().unwrap();
    }

    #[test]
    fn vcan0_test_nonblocking() {
        let cs = CanSocket::open("vcan0").unwrap();
        cs.set_nonblocking(true);

        // no timeout set, but should return immediately
        assert!(cs.read_frame().should_retry());
    }
}
