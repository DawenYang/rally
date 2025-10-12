use async_runtime::{executor::Executor, receiver::TcpReceiver, sender::TcpSender};
use data_layer::data::Data;
use std::{
    io,
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Instant,
};

async fn send_data_async(field1: u32, field2: u16, field3: String) -> io::Result<String> {
    println!("Client: Connecting to server...");
    let stream = Arc::new(Mutex::new(TcpStream::connect("127.0.0.1:7878")?));
    println!("Client: Connected!");

    let message = Data {
        field1,
        field2,
        field3,
    };

    println!("Client: Serializing and sending data...");
    TcpSender {
        stream: stream.clone(),
        buffer: message.serialize()?,
        written: 0,
    }
    .await?;
    println!("Client: Data sent!");

    println!("Client: Waiting for response...");
    let receiver = TcpReceiver {
        stream: stream.clone(),
        buffer: Vec::new(),
    };

    let response = receiver.await?;
    println!("Client: Received {} bytes", response.len());

    String::from_utf8(response)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))
}

pub fn send_data() -> io::Result<()> {
    let mut executor = Executor::new();
    let mut handlers = Vec::new();
    let start = Instant::now();
    for i in 0..40 {
        let handle = executor.spawn(send_data_async(
            i,
            i as u16,
            format!("Hello, server! {}", i),
        ));
        handlers.push(handle);
    }
    println!("Client: Starting executor loop...");
    std::thread::spawn(move || loop {
        executor.poll();
    });
    for handle in handlers {
        match handle.recv().unwrap() {
            Ok(result) => println!("Result: {}", result),
            Err(e) => println!("Error: {}", e),
        };
    }
    let duration = start.elapsed();
    print!("Time elapsed in sending data to server is: {:?}", duration);
    Ok(())
}
