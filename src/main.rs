use std::iter;
use std::thread;
use std::process::Command;
use std::io::{self, Write, Error};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
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
    let mut next_jid: usize = 1;
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
        let arc = arc.clone();
        input = thread::spawn(move || {
            loop {
                print!("RustShell> ");
                io::stdout().flush().unwrap();

                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer).expect("could not read from stdin");

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
                    "quit\n" => break,
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

                        if let Ok(mut child) = cmd.spawn() {
                            let mut jobs = arc.lock().unwrap();
                            jobs[next_jid-1] = Job { jid: next_jid, pid: child.id(), state: state, cmdline: buffer };
                            next_jid = (next_jid % MAX_JID) + 1;
                            if state == State::FG {
                                child.wait().expect("bad");
                            }
                        } else {
                            println!("invalid command {} {}", path.trim_end(), args.trim_end());
                        }
                    }
                };
            } 
        });
    }
    
    for info in &mut signals {
        match info.si_signo {
            SIGINT => return Ok(()),
            SIGTSTP => println!("sigstp"),
            SIGCONT => println!("sigcnt"),
            SIGCHLD => {
                let jobs = arc.lock().unwrap();
                for job in &*jobs {
                    match waitpid(Pid::from_raw(job.pid as i32), Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::StillAlive) => {},
                        Ok(status) => { 
                            println!("job {} {:?}", job.jid, status);
                            /*{
                                let mut jobs = arc.lock().unwrap();
                                jobs[job.pid as usize] = Job { jid: 0, pid: 0, state: State::UNDEF, cmdline: String::new() };
                                println!("job {} {:?}", job.jid, status);
                            }*/
                        },
                        Err(_e) => {}
                    }
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
