use std::{
    io::{Read, Result},
    net::TcpStream,
    sync::mpsc::{SyncSender, TrySendError},
};

use crate::{
    config::jpeg_dimensions,
    error::SessionError,
    tunnel::{MAX_FRAME_BYTES, Shared},
};

pub(crate) fn read_frames(mut stream: TcpStream, sender: SyncSender<Vec<u8>>, shared: &Shared) {
    loop {
        let length = match read_frame_length(&mut stream) {
            Ok(Some(length)) => u32::from_le_bytes(length) as usize,
            Ok(None) => {
                shared.finish_eof();
                return;
            }
            Err(error) => {
                shared.finish_failure(SessionError::read("reading a frame length", error));
                return;
            }
        };
        if length == 0 || length > MAX_FRAME_BYTES {
            shared.finish_failure(SessionError::InvalidFrameLength(length));
            return;
        }
        let mut frame = vec![0_u8; length];
        if let Err(error) = stream.read_exact(&mut frame) {
            shared.finish_failure(SessionError::read("reading a frame body", error));
            return;
        }
        let Some((width, height)) = jpeg_dimensions(&frame) else {
            shared.finish_failure(SessionError::InvalidJpeg);
            return;
        };
        shared.publish_dimensions(width, height);
        match sender.try_send(frame) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                shared.finish_eof();
                return;
            }
        }
    }
}

fn read_frame_length(stream: &mut TcpStream) -> Result<Option<[u8; 4]>> {
    let mut length = [0_u8; 4];
    let read = stream.read(&mut length)?;
    if read == 0 {
        return Ok(None);
    }
    stream.read_exact(&mut length[read..])?;
    Ok(Some(length))
}
