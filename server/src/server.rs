use std::{
    io::{self, Cursor, ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use async_runtime::{executor::Executor, sleep::Sleep};
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

pub fn start_server(addr: &str) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Server listening on {}", addr);

    let (one_tx, one_rx) = mpsc::channel::<TcpStream>();
    let (two_tx, two_rx) = mpsc::channel::<TcpStream>();
    let (three_tx, three_rx) = mpsc::channel::<TcpStream>();

    // Spawn 3 worker threads
    let worker1 = spawn_worker!("Worker-1", one_rx, &FLAGS[0]);
    let worker2 = spawn_worker!("Worker-2", two_rx, &FLAGS[1]);
    let worker3 = spawn_worker!("Worker-3", three_rx, &FLAGS[2]);

    let router = [one_tx, two_tx, three_tx];
    let worker_pool = [worker1, worker2, worker3];
    let mut index = 0;

    // Accept connections and distribute to workers
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = router[index].send(stream);
                if FLAGS[index].load(Ordering::SeqCst) {
                    FLAGS[index].store(false, Ordering::SeqCst);
                    worker_pool[index].thread().unpark();
                }
                index += 1;
                if index == 3 {
                    index = 0;
                }
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }
    Ok(())
}
