use std::{env, thread, time::Duration};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage:");
        println!("  rally server       - Start the server");
        println!("  rally client       - Send a test message to the server");
        return;
    }

    match args[1].as_str() {
        "server" => {
            println!("Starting Rally server...");
            if let Err(e) = server::server::start_server("127.0.0.1:7878") {
                eprintln!("Server error: {}", e);
            }
        }
        "client" => {
            println!("Sending message to server...");
            // Give a moment for the server to be ready if just started
            thread::sleep(Duration::from_millis(100));

            match client::client::send_data() {
                Ok(_) => {
                    println!("Client flushed out response from server");
                }
                Err(e) => {
                    eprintln!("Client error: {}", e);
                }
            }
        }
        _ => {
            println!("Unknown command: {}", args[1]);
            println!("Use 'server' or 'client'");
        }
    }
}
