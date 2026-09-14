use super::super::errors::{HttpError, HttpErrorKind};

#[derive(Debug, Clone)]
pub struct FlowControl {
    pub connection_send_window: i32,
    pub connection_recv_window: i32,
    pub initial_stream_window: u32,
    pub max_frame_size: u32,
}

impl Default for FlowControl {
    fn default() -> Self {
        Self {
            connection_send_window: 65535,
            connection_recv_window: 65535,
            initial_stream_window: 65535,
            max_frame_size: 16384,
        }
    }
}

impl FlowControl {
    pub fn update_send_window(&mut self, increment: u32) -> Result<(), HttpError> {
        let new_window = self.connection_send_window as i64 + increment as i64;
        if new_window > 2147483647 {
            return Err(HttpError::new(
                HttpErrorKind::Http2Error,
                "Flow control window overflow (> 2^31 - 1)",
            ));
        }
        self.connection_send_window = new_window as i32;
        Ok(())
    }

    pub fn consume_send_window(&mut self, amount: usize) -> Result<(), HttpError> {
        self.connection_send_window -= amount as i32;
        Ok(())
    }
}
