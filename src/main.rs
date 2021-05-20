use std::{iter, thread, time};
use std::process::{Child, Command};
use std::io::{self, Write, Error};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

use signal_hook::consts::signal::*;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::flag;
use signal_hook::iterator::SignalsInfo;
use signal_hook::iterator::exfiltrator::WithRawSiginfo;

#[derive(PartialEq, Eq, Copy, Clone)]
enum State {
    UNDEF,
    FG,
    BG,
    ST,
}

struct Job {
    jid: usize,
    pid: u32,
    state: State,
    cmdline: String,
}

fn state_str(state: &State) -> String {
    match state {
        State::FG => return "Running".to_string(), 
        State::BG => return "Running".to_string(),
        State::ST => return "Stopped".to_string(),
        _ => return "Undefined".to_string()
    };
}

fn help_msg() {
    print!("\nRustShell Commands\n\n");

    println!("<path> <args>: runs the program at <path> with arguments <args>");
    println!("jobs: lists stopped jobs and running background jobs");
    println!("bg <job>: change a stopped job to a running background job");
    println!("fg <job>: change a running background job to a foreground job\n");
}

fn main() -> Result<(), Error> {
    const MAX_JID: usize = 1 << 16;
    let jobs: Vec<Job> = iter::repeat_with(|| Job { jid: 0, pid: 0, state: State::UNDEF, cmdline: String::new() }).take(MAX_JID).collect();
    let arc = Arc::new(Mutex::new(jobs));

    let term_now = Arc::new(AtomicBool::new(false));
    for sig in TERM_SIGNALS {
        flag::register_conditional_shutdown(*sig, 1, Arc::clone(&term_now))?;
        flag::register(*sig, Arc::clone(&term_now))?;
    }

    let mut sigs = vec![SIGTSTP, SIGCONT, SIGINT, SIGCHLD];
    sigs.extend(TERM_SIGNALS);
    let mut signals = SignalsInfo::<WithRawSiginfo>::new(&sigs)?;

    let input;
    {
        let mut next_jid: usize = 1;
        let arc = arc.clone();

        input = thread::spawn(move || {
            let mut children: Vec<Child> = Vec::with_capacity(MAX_JID);
            loop {
                print!("RustShell> ");
                io::stdout().flush().unwrap();

                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer).expect("could not read from stdin");

                match &buffer[..2] {
                    "bg" | "fg"  => {
                        let mut jobs = arc.lock().unwrap();
                        match buffer[3..].trim_end().parse::<usize>() {
                            Ok(jid) => {
                                if jobs[jid-1].state == State::UNDEF {
                                    eprintln!("error: job with id {} not found", jid);
                                } else {
                                    jobs[jid-1].state = if buffer[..2].eq("bg") { State::BG } else { State::FG };
                                    signal::kill(Pid::from_raw(jobs[jid-1].pid as i32), Signal::SIGCONT).unwrap();
                                }
                            },
                            Err(_e) => eprintln!("error: bg command must have format bg <job>")
                        }
                    },
                    _ => {
                        match &buffer[..] {
                            "help\n" => help_msg(),
                            "jobs\n" => {
                                let jobs = arc.lock().unwrap();
                                for job in &*jobs {
                                    if job.state != State::UNDEF {
                                        println!("{}\t{} {}\t\t{}", job.jid, job.pid, state_str(&job.state), job.cmdline.trim_end());
                                    }
                                }
                            },
                            _ => {
                                let (path, args) = buffer.split_once(' ').unwrap_or((&buffer, ""));
                                let mut cmd = Command::new(path.trim_end());
                                let mut state = State::FG;
                                if args.len() > 0 {
                                    if args.trim_end().chars().last().unwrap() == '&' {
                                        state = State::BG;
                                    }
                                    let cmd_args = if state == State::BG { args.replace(" &\n", "\n") } else { args.to_string() };
                                    if cmd_args.len() > 0 {
                                        cmd.args(cmd_args.split(' ').map(|arg| arg.trim_end()).collect::<Vec<&str>>());
                                    }
                                }

                                if let Ok(child) = cmd.spawn() {
                                    let mut jobs = arc.lock().unwrap();
                                    jobs[next_jid-1] = Job { jid: next_jid, pid: child.id(), state: state, cmdline: buffer };
                                    children.insert(next_jid-1, child);
                                    next_jid = (next_jid % MAX_JID) + 1;
                                } else {
                                    println!("invalid command {} {}", path.trim_end(), args.trim_end());
                                }
                            }
                        };
                    }
                }

                loop {
                    let mut fg_active = false;
                    let jobs = arc.lock().unwrap();
                    for job in &*jobs {
                        if job.state == State::FG {
                            fg_active = true;
                        }
                    }
                    if fg_active {
                        thread::sleep(time::Duration::from_millis(1));
                    } else {
                        break;
                    }
                }
            } 
        });
    }
    
    for info in &mut signals {
        match info.si_signo {
            SIGINT => {
                let mut jid = 0;
                let mut jobs = arc.lock().unwrap();
                for job in &*jobs {
                    if job.state == State::FG {
                        jid = job.jid;
                        println!("\nJob [{}] ({}) terminated by signal SIGINT", job.jid, job.pid);
                        break;
                    }
                } 
                if jid > 0 {
                    jobs[jid-1].state = State::UNDEF;
                } else {
                    return Ok(());
                }
            },
            SIGTSTP => {
                let mut jid = 0;
                let mut jobs = arc.lock().unwrap();
                for job in &*jobs {
                    if job.state == State::FG {
                        jid = job.jid;
                        signal::kill(Pid::from_raw(job.pid as i32), Signal::SIGTSTP).unwrap();
                        println!("\nJob [{}] ({}) stopped by signal SIGTSTP", job.jid, job.pid);
                        break;
                    }
                }
                if jid > 0 {
                    jobs[jid-1].state = State::ST;
                }
            },
            SIGCHLD => {
                let mut jid = 0;
                let mut jobs = arc.lock().unwrap();
                for job in &*jobs {
                    match waitpid(Pid::from_raw(job.pid as i32), Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::StillAlive) => {},
                        Ok(_status) => {
                            jid = job.jid;
                            break;
                        },
                        Err(_e) => {}
                    }
                }
                if jid > 0 {
                    jobs[jid-1].state = State::UNDEF;
                }
            },
            term_sig => {
                eprintln!("error: unhandled signal");
                assert!(TERM_SIGNALS.contains(&term_sig));
                break;
            }
        }
    }

    input.join().unwrap();

    Ok(())
}
