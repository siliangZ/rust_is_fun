#[macro_use]
extern crate lazy_static;
use bincode::{deserialize, serialize_into};
use cpu_time::ProcessTime;
use log::trace;
use nix::fcntl::{open, OFlag};
use nix::libc::{fcntl, getpid, signal, F_GETFL, F_SETFL, F_SETOWN, O_ASYNC, SIGIO};
use nix::sys::stat::Mode;
use nix::unistd::{read, write};
use rpmsg_async_notify::ffi::clock;
use rpmsg_async_notify::remote_proc::RemoteprocManager;
use rpmsg_async_notify::{
    prepare_environment, receive_tick, receive_tick_instant, send_tick, send_tick_instant, Payload,
    NUM_PAYLOADS, PAYLOAD_MAX_SIZE,
};
use std::env;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};
lazy_static! {
    static ref endpoint_fd: Mutex<Option<i32>> = Mutex::new(None);
}

pub fn sigio_handler(_: i32) {
    unsafe {
        let fd = endpoint_fd.lock().unwrap();
        if let Some(fd) = *fd {
            //let time_stamp = clock();
            let time_stamp = Instant::now();
            let mut buf = [0u8; 1024];
            let bytes_rcvd = read(fd, buf.as_mut()).unwrap();
            //let raw_pointer = buf.as_ptr() as *const Payload;
            let r_payload: Payload = deserialize(&buf).unwrap();
            {
                let mut r_tick = receive_tick_instant.lock().unwrap();
                r_tick.insert(r_payload.num as usize, time_stamp);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let num_payloads = if args.len() > 1 {
        args[1]
            .parse::<usize>()
            .expect("please use a valid parameter")
    } else {
        1_000_000
    };
    let remote_proc = RemoteprocManager::new("remoteproc0").unwrap();
    remote_proc
        .load_firmware("echo_test.elf".to_string())
        .unwrap();
    remote_proc.start();

    let endpoint_path = prepare_environment();
    // register signal handler
    unsafe {
        {
            let mut fd = endpoint_fd.lock().unwrap();
            *fd = if endpoint_path.exists() {
                trace!("opening endpoint handler");
                Some(
                    open(
                        &endpoint_path,
                        OFlag::O_RDWR | OFlag::O_NONBLOCK,
                        Mode::empty(),
                    )
                    .unwrap(),
                )
            } else {
                panic!("can't find endpoint in the system");
            };

            signal(SIGIO, sigio_handler as usize); // libc method to register a handler to a signal

            fcntl((*fd).unwrap(), F_SETOWN, getpid()); // Tell the kernel to whom to send the signal? Reflected by PID number
            let current_flags = fcntl((*fd).unwrap(), F_GETFL); // The application program reads the flag bit Oflags
            fcntl((*fd).unwrap(), F_SETFL, current_flags | O_ASYNC);
        }

        // send a payload over
        for id in 0..num_payloads {
            let payload = Payload {
                num: id as u64,
                size: 20,
                data: vec![10; 20],
            };
            let mut sent_buf = [0u8; 1024];
            serialize_into(sent_buf.as_mut(), &payload).unwrap();
            {
                let fd = endpoint_fd.lock().unwrap();
                let bytes_sent = write((*fd).unwrap(), &sent_buf[..PAYLOAD_MAX_SIZE]).unwrap();
            }

            let mut tick_array = send_tick_instant.lock().unwrap();
            tick_array.insert(id, Instant::now());

            //tick_array.insert(id, clock());

            //println!("sent out {} bytes", bytes_sent);
            sleep(Duration::from_millis(100));
        }
        sleep(Duration::from_secs(1));

        // calculate the average delay
        let r_tick = receive_tick_instant.lock().unwrap();
        let s_tick = send_tick_instant.lock().unwrap();
        //println!("[debug] r_tick: {:?}", r_tick);
        //println!("[debug] s_tick: {:?}", s_tick);
        let mut counter = 0;

        let mut total_diff = Duration::from_millis(0);
        let mut max_diff = Duration::from_millis(u64::MIN);
        let mut min_diff = Duration::from_millis(u64::MAX);
        //let mut total_diff = 0;
        //let mut max_diff = i64::MIN;
        //let mut min_diff = i64::MAX;
        for (id, receive_time) in r_tick.iter() {
            //println!("id: {}", id);
            if let Some(send_time) = s_tick.get(id) {
                //println!(
                //"message: {}, receive_time:{:?}, send_time: {:?}",
                //id, receive_time, send_time
                //);
                if receive_time < send_time {
                    println!(
                        "[error] id:{:?}, receive_time:{:?}, send_time: {:?}",
                        id, receive_time, send_time
                    );
                    continue;
                }
                let diff = receive_time.duration_since(send_time.clone());
                //let diff = receive_time - send_time;

                if diff > Duration::from_secs(1) {
                    println!(
                        "message: {:?}, receive_time: {:?}, send_time: {:?}, diff: {:?}",
                        id, receive_time, send_time, diff
                    );
                }
                //println!("time diff: {:?}", diff);
                if diff > max_diff {
                    max_diff = diff;
                }
                if diff < min_diff {
                    min_diff = diff;
                }
                total_diff += diff;
                counter += 1;
            }
        }
        println!("number of payload: {}", counter);
        println!("max delay: {:?}", max_diff);
        println!("min delay: {:?}", min_diff);
        println!("average delay: {:?}", total_diff.div_f32(counter as f32));
        //println!("average delay: {:?}", total_diff as f32 / counter as f32);
        remote_proc.stop().unwrap();
    }
}
