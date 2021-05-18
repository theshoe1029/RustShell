use std::ffi::CString;
use std::thread;
use std::io::{self, Write, Error};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use nix::unistd;

use signal_hook::consts::signal::*;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::flag;
use signal_hook::iterator::SignalsInfo;
use signal_hook::iterator::exfiltrator::WithRawSiginfo;

fn help_msg() {
    print!("\nRustShell Commands\n\n");

    println!("<path> <args>: runs the program at <path> with arguments <args>");
    println!("jobs: lists stopped jobs and running background jobs");
    println!("bg <job>: change a stopped job to a running background job");
    println!("fg <job>: change a running background job to a foreground job\n");
}

fn list_jobs() {
}

fn main() -> Result<(), Error> {
    let term_now = Arc::new(AtomicBool::new(false));
    for sig in TERM_SIGNALS {
        flag::register_conditional_shutdown(*sig, 1, Arc::clone(&term_now))?;
        flag::register(*sig, Arc::clone(&term_now))?;
    }

    let mut sigs = vec![SIGTSTP, SIGCONT, SIGINT];
    sigs.extend(TERM_SIGNALS);
    let mut signals = SignalsInfo::<WithRawSiginfo>::new(&sigs)?;

    let input = thread::spawn(|| -> Result<(), Error> {
        loop {
            print!("RustShell> ");
            io::stdout().flush().unwrap();

            let mut buffer = String::new();
            io::stdin().read_line(&mut buffer)?;

            match &buffer[..] {
                "help\n" => help_msg(),
                "jobs\n" => list_jobs(),
                _ => {
                    let (path, args) = buffer.split_once(' ').unwrap_or((&buffer, ""));
                    let cpath: CString = CString::new(path)?;
                    let cenv: &[CString] = &[];
//                    unistd::execve(&cpath, args.split(' ').map(|arg| CString::new(arg).unwrap()).collect::<Vec<CString>>().as_slice(), cenv);
                    unistd::execve(&CString::new("/bin/ls")?, &[&CString::new("/bin/ls")?, NULL], cenv);
                }
            };
        }
    });

    for info in &mut signals {
        match info.si_signo {
            SIGINT => return Ok(()),
            SIGTSTP => println!("sigstp"),
            SIGCONT => println!("sigcnt"),
            term_sig => {
                eprintln!("error: unhandled signal");
                assert!(TERM_SIGNALS.contains(&term_sig));
                break;
            }
        }
    }

    input.join().unwrap()?;

    Ok(())
}
