use async_runtime::{receiver::TcpReceiver, sender::TcpSender};
use data_layer::data::Data;
use std::{
    io,
    net::TcpStream,
    sync::{Arc, Mutex},
};

async fn send_data(field1: u32, field2: u16, field3: String) -> io::Result<String> {
    let stream = Arc::new(Mutex::new(TcpStream::connect("127.0.0.1:7878")?));

    let message = Data {
        field1,
        field2,
        field3,
    };

    TcpSender {
        stream: stream.clone(),
        buffer: message.serialize()?,
    }
    .await?;

    let receiver = TcpReceiver {
        stream: stream.clone(),
        buffer: Vec::new(),
    };

    String::from_utf8(receiver.await?)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))
}
