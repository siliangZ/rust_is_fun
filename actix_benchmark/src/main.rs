extern crate actix;
extern crate futures;

use actix::*;

use std::env;

/// A payload with a counter
struct Payload(usize);

impl Message for Payload {
    type Result = ();
}

struct Node { 
    limit: usize,
    next: Recipient<Unsync, Payload>,
}

impl Actor for Node {
    type Context = Context<Self>;
}

impl Handler<Payload> for Node {
    type Result = ();

    fn handle(&mut self, msg: Payload, _: &mut Context<Self>) {
        if msg.0 >= self.limit {
            println!("Reached limit of {} (payload was {})", self.limit, msg.0);
            Arbiter::system().do_send(actix::msgs::SystemExit(0));
            return;
        }
        self.next.do_send(Payload(msg.0 + 1)).expect("Unable to send payload");
    }
}

fn print_usage_and_exit() {
    eprintln!("Usage; actix-test <num-nodes> <num-times-message-around-ring>");
    ::std::process::exit(1);
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if  args.len() < 3 {
        print_usage_and_exit();
    }
    let mut num_nodes = 2; 
    if let Ok(arg_num_nodes) = args[1].parse::<usize>() {
        if arg_num_nodes <= 1 {
            eprintln!("Number of nodes must be > 1");
            ::std::process::exit(1);
        }
        num_nodes = arg_num_nodes;
    } else {
        print_usage_and_exit();
    }
    let num_nodes = num_nodes;

    let mut ntimes = 1;
    if let Ok(arg_ntimes) = args[2].parse::<usize>() {
        ntimes = arg_ntimes;
    } else {
        print_usage_and_exit();
    }
    let ntimes = ntimes; // make constant;

    let system = System::new("test");

    println!("Setting up nodes");
    let _: Addr<Unsync, _> = Node::create(move|ctx| {
        let first_addr : Addr<Unsync, _> = ctx.address();
        let mut prev_addr: Addr<Unsync, _> = Node{limit: num_nodes * ntimes, next: first_addr.recipient()}.start();
        prev_addr.do_send(Payload(0));

        for _ in 2..num_nodes {
            prev_addr = Node{limit: num_nodes * ntimes, next: prev_addr.recipient()}.start();
        }

        Node{limit: num_nodes * ntimes, next: prev_addr.recipient()}
    });

    system.run();
}