use libc::{
    c_int, c_short, c_uint, c_void, close, connect, fcntl, read, sockaddr, socket, timeval, write,
    F_SETFL, O_NONBLOCK,
};

use futures;
// use mio::{Evented, PollOpt, Ready, Token};
use nix::net::if_::if_nametoindex;
use std::collections::VecDeque;
use std::fmt;
use std::io::{Error, ErrorKind};
use std::mem::size_of;
use std::{io, slice, time};
use romio::PollEvented;

use std::pin::Pin;

use futures::stream::Stream;
use futures::task::LocalWaker;
use futures::{ready, Poll};
use mio;


use socketcan::{
    c_timeval_new, CanMessageId, CanAddr, CanFrame, CanSocketOpenError, FrameFlags, AF_CAN, CAN_BCM, PF_CAN,
    SOCK_DGRAM,
};

pub const MAX_NFRAMES: u32 = 256;

/// OpCodes
///
/// create (cyclic) transmission task
pub const TX_SETUP: u32 = 1;
/// remove (cyclic) transmission task
pub const TX_DELETE: u32 = 2;
/// read properties of (cyclic) transmission task
pub const TX_READ: u32 = 3;
/// send one CAN frame
pub const TX_SEND: u32 = 4;
/// create RX content filter subscription
pub const RX_SETUP: u32 = 5;
/// remove RX content filter subscription
pub const RX_DELETE: u32 = 6;
/// read properties of RX content filter subscription
pub const RX_READ: u32 = 7;
/// reply to TX_READ request
pub const TX_STATUS: u32 = 8;
/// notification on performed transmissions (count=0)
pub const TX_EXPIRED: u32 = 9;
/// reply to RX_READ request
pub const RX_STATUS: u32 = 10;
/// cyclic message is absent
pub const RX_TIMEOUT: u32 = 11;
/// sent if the first or a revised CAN message was received
pub const RX_CHANGED: u32 = 12;

/// Flags
///
/// set the value of ival1, ival2 and count
pub const SETTIMER: u32 = 0x0001;
/// start the timer with the actual value of ival1, ival2 and count.
/// Starting the timer leads simultaneously to emit a can_frame.
pub const STARTTIMER: u32 = 0x0002;
/// create the message TX_EXPIRED when count expires
pub const TX_COUNTEVT: u32 = 0x0004;
/// A change of data by the process is emitted immediatly.
/// (Requirement of 'Changing Now' - BAES)
pub const TX_ANNOUNCE: u32 = 0x0008;
/// Copies the can_id from the message header to each subsequent frame
/// in frames. This is intended only as usage simplification.
pub const TX_CP_CAN_ID: u32 = 0x0010;
/// Filter by can_id alone, no frames required (nframes=0)
pub const RX_FILTER_ID: u32 = 0x0020;
/// A change of the DLC leads to an RX_CHANGED.
pub const RX_CHECK_DLC: u32 = 0x0040;
/// If the timer ival1 in the RX_SETUP has been set equal to zero, on receipt
/// of the CAN message the timer for the timeout monitoring is automatically
/// started. Setting this flag prevents the automatic start timer.
pub const RX_NO_AUTOTIMER: u32 = 0x0080;
/// refers also to the time-out supervision of the management RX_SETUP.
/// By setting this flag, when an RX-outs occours, a RX_CHANGED will be
/// generated when the (cyclic) receive restarts. This will happen even if the
/// user data have not changed.
pub const RX_ANNOUNCE_RESUM: u32 = 0x0100;
/// forces a reset of the index counter from the update to be sent by multiplex
/// message even if it would not be necessary because of the length.
pub const TX_RESET_MULTI_ID: u32 = 0x0200;
/// the filter passed is used as CAN message to be sent when receiving an RTR frame.
pub const RX_RTR_FRAME: u32 = 0x0400;
pub const CAN_FD_FRAME: u32 = 0x0800;

/// BcmMsgHead
///
/// Head of messages to and from the broadcast manager
#[repr(C)]
pub struct BcmMsgHead {
    _opcode: u32,
    _flags: u32,
    /// number of frames to send before changing interval
    _count: u32,
    /// interval for the first count frames
    _ival1: timeval,
    /// interval for the following frames
    _ival2: timeval,
    _can_id: u32,
    /// number of can frames appended to the message head
    _nframes: u32,
    // TODO figure out how why C adds a padding here?
    #[cfg(all(target_pointer_width = "32"))]
    _pad: u32,
    // TODO figure out how to allocate only nframes instead of MAX_NFRAMES
    /// buffer of CAN frames
    _frames: [CanFrame; MAX_NFRAMES as usize],
}

impl fmt::Debug for BcmMsgHead {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BcmMsgHead {{ _opcode: {}, _flags: {} , _count: {}, _ival1: {:?}, _ival2: {:?}, _can_id: {}, _nframes: {}}}", self._opcode, self._flags,              self._count, self._ival1.tv_sec, self._ival2.tv_sec, self._can_id, self._nframes)
    }
}

/// BcmMsgHeadFrameLess
///
/// Head of messages to and from the broadcast manager see _pad fields for differences
/// to BcmMsgHead
#[repr(C)]
pub struct BcmMsgHeadFrameLess {
    _opcode: u32,
    _flags: u32,
    /// number of frames to send before changing interval
    _count: u32,
    /// interval for the first count frames
    _ival1: timeval,
    /// interval for the following frames
    _ival2: timeval,
    _can_id: u32,
    /// number of can frames appended to the message head
    _nframes: u32,
    // Workaround Rust ZST has a size of 0 for frames, in
    // C the BcmMsgHead struct contains an Array that although it has
    // a length of zero still takes n (4) bytes.
    #[cfg(all(target_pointer_width = "32"))]
    _pad: usize,
}

#[repr(C)]
pub struct TxMsg {
    _msg_head: BcmMsgHeadFrameLess,
    _frames: [CanFrame; MAX_NFRAMES as usize],
}

impl BcmMsgHead {
    pub fn can_id(&self) -> u32 {
        self._can_id
    }

    #[inline]
    pub fn frames(&self) -> &[CanFrame] {
        return unsafe { slice::from_raw_parts(self._frames.as_ptr(), self._nframes as usize) };
    }
}

/// A socket for a CAN device, specifically for broadcast manager operations.
#[derive(Debug)]
pub struct CanBCMSocket {
    pub fd: c_int,
}

pub struct BcmFrameStream {
    io: PollEvented<CanBCMSocket>,
    frame_buffer: VecDeque<CanFrame>,
}

impl BcmFrameStream {
    pub fn new(socket: CanBCMSocket) -> BcmFrameStream {
        BcmFrameStream {
            io: PollEvented::new(socket),
            frame_buffer: VecDeque::new(),
        }
    }
}

impl Stream for BcmFrameStream {
    type Item = io::Result<CanFrame>;

    fn poll_next(mut self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Option<Self::Item>> {
        // Buffer still contains frames
        // after testing this it looks like the recv_msg will never contain
        // more than one msg, therefore the buffer is basically never filled

        if let Some(frame) = self.frame_buffer.pop_front() {
            return Poll::Ready(Some(Ok(frame)));
        }

        ready!(self.io.poll_read_ready(lw)?);

        match self.io.get_ref().read_msg() {
            Ok(n) => {
                let mut frames = n.frames().to_vec();
                if let Some(frame) = frames.pop() {
                    if !frames.is_empty() {
                        for frame in n.frames() {
                            self.frame_buffer.push_back(*frame)
                        }
                    }
                    Poll::Ready(Some(Ok(frame)))
                } else {
                    // This happens e.g. when a timed out msg is received
                    self.io.clear_read_ready(lw)?;
                    Poll::Pending
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.clear_read_ready(lw)?;
                    return Poll::Pending;
                }
                return Poll::Ready(Some(Err(e)));
            }
        }
    }
}

impl mio::Evented for BcmFrameStream {
    fn register(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        self.io.get_ref().register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        self.io.get_ref().reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        self.io.get_ref().deregister(poll)
    }
}

impl CanBCMSocket {
    /// Open a named CAN device non blocking.
    ///
    /// Usually the more common case, opens a socket can device by name, such
    /// as "vcan0" or "socan0".
    pub fn open_nb(ifname: &str) -> Result<CanBCMSocket, CanSocketOpenError> {
        let if_index = if_nametoindex(ifname)?;
        CanBCMSocket::open_if_nb(if_index)
    }

    /// Open CAN device by interface number non blocking.
    ///
    /// Opens a CAN device by kernel interface number.
    pub fn open_if_nb(if_index: c_uint) -> Result<CanBCMSocket, CanSocketOpenError> {
        // open socket
        let sock_fd;
        unsafe {
            sock_fd = socket(PF_CAN, SOCK_DGRAM, CAN_BCM);
        }

        if sock_fd == -1 {
            return Err(CanSocketOpenError::from(io::Error::last_os_error()));
        }

        let fcntl_resp = unsafe { fcntl(sock_fd, F_SETFL, O_NONBLOCK) };

        if fcntl_resp == -1 {
            return Err(CanSocketOpenError::from(io::Error::last_os_error()));
        }

        let addr = CanAddr {
            _af_can: AF_CAN as c_short,
            if_index: if_index as c_int,
            rx_id: 0, // ?
            tx_id: 0, // ?
        };

        let sockaddr_ptr = &addr as *const CanAddr;

        let connect_res;
        unsafe {
            connect_res = connect(
                sock_fd,
                sockaddr_ptr as *const sockaddr,
                size_of::<CanAddr>() as u32,
            );
        }

        if connect_res != 0 {
            return Err(CanSocketOpenError::from(io::Error::last_os_error()));
        }

        Ok(CanBCMSocket { fd: sock_fd })
    }

    fn close(&mut self) -> io::Result<()> {
        unsafe {
            let rv = close(self.fd);
            if rv != -1 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Create a content filter subscription, filtering can frames by can_id.
    pub fn filter_id(
        &self,
        can_id: CanMessageId,
        ival1: time::Duration,
        ival2: time::Duration,
    ) -> io::Result<()> {
        let _ival1 = c_timeval_new(ival1);
        let _ival2 = c_timeval_new(ival2);

        let frames = [CanFrame::new(CanMessageId::SFF(0u16), &[], false, false).unwrap(); MAX_NFRAMES as usize];
        let msg = BcmMsgHeadFrameLess {
            _opcode: RX_SETUP,
            _flags: SETTIMER | RX_FILTER_ID,
            _count: 0,
            #[cfg(all(target_pointer_width = "32"))]
            _pad: 0,
            _ival1: _ival1,
            _ival2: _ival2,
            _can_id: can_id.with_eff_bit(),
            _nframes: 0,
        };

        let tx_msg = &TxMsg {
            _msg_head: msg,
            _frames: frames,
        };

         let tx_msg_ptr = tx_msg as *const TxMsg;

        let write_rv = unsafe {
            write(self.fd, tx_msg_ptr as *const c_void, size_of::<TxMsg>())
        };

        if write_rv < 0 {
            return Err(Error::new(ErrorKind::WriteZero, io::Error::last_os_error()));
        }

        Ok(())
    }

    ///
    /// Combination of `CanBCMSocket::filter_id` and `CanBCMSocket::incoming_frames`.
    /// ```
    /// extern crate futures;
    /// extern crate tokio;
    /// extern crate socketcan;
    ///
    /// use futures::stream::Stream;
    /// use tokio::prelude::*;
    /// use std::time;
    /// use socketcan::FrameFlags;
    /// use socketcan_tokio::bcm::*;
    ///
    /// let ival = time::Duration::from_millis(1);
    /// let socket = CanBCMSocket::open_nb("vcan0").unwrap();
    /// let f = socket.filter_id_incoming_frames(0x123, ival, ival, FrameFlags::EFF_FLAG).unwrap()
    ///       .map_err(|_| ())
    ///       .for_each(|frame| {
    ///          println!("Frame {:?}", frame);
    ///          Ok(())
    ///        });
    /// tokio::run(f);
    /// ```
    ///
    pub fn filter_id_incoming_frames(
        self,
        can_id: CanMessageId,
        ival1: time::Duration,
        ival2: time::Duration,
    ) -> io::Result<BcmFrameStream> {
        self.filter_id(can_id, ival1, ival2)?;
        Ok(self.incoming_frames())
    }

    ///
    /// Stream of incoming BcmMsgHeads that apply to the filter criteria.
    /// ```
    /// extern crate futures;
    /// extern crate tokio;
    /// extern crate socketcan;
    ///
    /// use futures::stream::Stream;
    /// use tokio::prelude::*;
    /// use std::time;
    /// use socketcan::FrameFlags;
    /// use socketcan_tokio::bcm::*;
    ///
    /// let socket = CanBCMSocket::open_nb("vcan0").unwrap();
    /// let ival = time::Duration::from_millis(1);
    /// socket.filter_id(0x123, ival, ival, FrameFlags::EFF_FLAG).unwrap();
    /// let f = socket.incoming_msg()
    ///        .map_err(|err| {
    ///            eprintln!("IO error {:?}", err)
    ///        })
    ///       .for_each(|bcm_msg_head| {
    ///          println!("BcmMsgHead {:?}", bcm_msg_head);
    ///          Ok(())
    ///        });
    /// tokio::run(f);
    /// ```
    ///
    pub fn incoming_msg(self) -> BcmStream {
        BcmStream::from(self)
    }

    ///
    /// Stream of incoming frames that apply to the filter criteria.
    /// ```
    /// extern crate futures;
    /// extern crate tokio;
    /// extern crate socketcan;
    ///
    /// use futures::stream::Stream;
    /// use tokio::prelude::*;
    /// use std::time;
    /// use socketcan::FrameFlags;
    /// use socketcan_tokio::bcm::*;
    ///
    /// let socket = CanBCMSocket::open_nb("vcan0").unwrap();
    /// let ival = time::Duration::from_millis(1);
    /// socket.filter_id(0x123, ival, ival, FrameFlags::EFF_FLAG).unwrap();
    /// let f = socket.incoming_frames()
    ///        .map_err(|err| {
    ///            eprintln!("IO error {:?}", err)
    ///        })
    ///       .for_each(|frame| {
    ///          println!("Frame {:?}", frame);
    ///          Ok(())
    ///        });
    /// tokio::run(f);
    /// ```
    ///
    pub fn incoming_frames(self) -> BcmFrameStream {
        //        let stream = BcmStream::from(self);
        //        stream
        //            .map(move |bcm_msg_head| {
        //                let v: Vec<CanFrame> = bcm_msg_head.frames().to_owned();
        //                futures::stream::iter_ok::<_, io::Error>(v)
        //            })
        //            .flatten()
        BcmFrameStream::new(self)
    }

    /// Remove a content filter subscription.
    pub fn filter_delete(&self, can_id: CanMessageId) -> io::Result<()> {
        let frames = [CanFrame::new(CanMessageId::EFF(0x0), &[], false, false).unwrap(); MAX_NFRAMES as usize];

        let msg = &BcmMsgHead {
            _opcode: RX_DELETE,
            _flags: 0,
            _count: 0,
            _ival1: c_timeval_new(time::Duration::new(0, 0)),
            _ival2: c_timeval_new(time::Duration::new(0, 0)),
            _can_id: can_id.with_eff_bit(),
            _nframes: 0,
            #[cfg(all(target_pointer_width = "32"))]
            _pad: 0,
            _frames: frames,
        };

        let msg_ptr = msg as *const BcmMsgHead;
        let write_rv = unsafe {
            write(self.fd, msg_ptr as *const c_void, size_of::<BcmMsgHead>())
        };

        let expected_size = size_of::<BcmMsgHead>() - size_of::<[CanFrame; MAX_NFRAMES as usize]>();
        if write_rv as usize != expected_size {
            let msg = format!("Wrote {} but expected {}", write_rv, expected_size);
            return Err(Error::new(ErrorKind::WriteZero, msg));
        }

        Ok(())
    }

    /// Read a single can frame.
    pub fn read_msg(&self) -> io::Result<BcmMsgHead> {
        let ival1 = c_timeval_new(time::Duration::from_millis(0));
        let ival2 = c_timeval_new(time::Duration::from_millis(0));
        let frames = [CanFrame::new(CanMessageId::EFF(0x0), &[], false, false).unwrap(); MAX_NFRAMES as usize];
        let mut msg = BcmMsgHead {
            _opcode: 0,
            _flags: 0,
            _count: 0,
            _ival1: ival1,
            _ival2: ival2,
            _can_id: 0,
            _nframes: 0,
            #[cfg(all(target_pointer_width = "32"))]
            _pad: 0,
            _frames: frames,
        };

        let msg_ptr = &mut msg as *mut BcmMsgHead;
        let count = unsafe {
            read(
                self.fd.clone(),
                msg_ptr as *mut c_void,
                size_of::<BcmMsgHead>(),
            )
        };

        let last_error = io::Error::last_os_error();
        if count < 0 {
            Err(last_error)
        } else {
            Ok(msg)
        }
    }
}

impl mio::Evented for CanBCMSocket {
    fn register(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        mio::unix::EventedFd(&self.fd).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        mio::unix::EventedFd(&self.fd).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        mio::unix::EventedFd(&self.fd).deregister(poll)
    }
}

impl Drop for CanBCMSocket {
    fn drop(&mut self) {
        self.close().ok(); // ignore result
    }
}

pub struct BcmStream {
    io: PollEvented<CanBCMSocket>,
}

pub trait IntoBcmStream {
    type Stream: futures::stream::Stream;
    type Error;

    fn into_bcm(self) -> Result<Self::Stream, Self::Error>;
}

impl BcmStream {
    pub fn from(bcm_socket: CanBCMSocket) -> BcmStream {
        let io = PollEvented::new(bcm_socket);
        BcmStream { io: io }
    }
}

impl Stream for BcmStream {
    type Item = io::Result<BcmMsgHead>;

    fn poll_next(self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Option<Self::Item>> {

        ready!(self.io.poll_read_ready(lw)?);

        match self.io.get_ref().read_msg() {
            Ok(n) => Poll::Ready(Some(Ok(n))),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.clear_read_ready(lw)?;
                    return Poll::Pending;
                }
                return Poll::Ready(Some(Err(e)));
            }
        }
    }
}
