use std::{
    io::{self, Cursor, ErrorKind, Read, Write},
    net::TcpStream,
    sync::atomic::AtomicBool,
    thread,
};

use async_runtime::sleep::Sleep;
use data_layer::data::Data;

static FLAGS: [AtomicBool; 3] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

macro_rules! spawn_worker {
    ($name:expr, $rx:expr, $flag:expr) => {
        thread::spawn(move || {
            let mut executor = Executor::new();
            loop {
                if let Ok(stream) = $rx.try_recv() {
                    println!(
                        "{} Received connection: {}",
                        $name,
                        stream.peer_addr().unwrap()
                    );
                    executor.spawn(handle_client(stream));
                } else {
                    if executor.polling.len() == 0 {
                        println!("{} is sleeping", $name);
                        $flag.store(true, Ordering::SeqCst);
                        thread::park();
                    }
                }
                executor.poll();
            }
        })
    };
}

async fn handle_client(mut stream: TcpStream) -> io::Result<()> {
    stream.set_nonblocking(true)?;
    let mut buffer = Vec::new();
    let mut local_buf = [0; 1024];

    loop {
        match stream.read(&mut local_buf) {
            Ok(0) => {
                break;
            }
            Ok(len) => {
                buffer.extend_from_slice(&local_buf[..len]);
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                if buffer.len() > 0 {
                    break;
                }
                Sleep::new(std::time::Duration::from_millis(10)).await;
                continue;
            }
            Err(e) => {
                println!("Failed to read from connection: {}", e);
            }
        }
    }
    match Data::deserialize(&mut Cursor::new(buffer.as_slice())) {
        Ok(messsage) => {
            println!("Received message: {:?}", messsage);
        }
        Err(e) => {
            println!("Failed to decode message: {}", e)
        }
    }
    Sleep::new(std::time::Duration::from_secs(1)).await;
    stream.write_all(b"Hello, client!")?;
    Ok(())
}
