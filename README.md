### Rust Shell

- Rust Shell runs basic Unix programs like /bin/ls, /bin/echo, and /bin/sleep well
- The shell hasn't hasn't been tested on more complex programs 
- This will likely not run on Windows because it uses Rust Unix bindings
- Run the shell for the first time using `cargo build && cargo run` and just `cargo run` after
- Once the shell is running the `help` command gives a full list of commands below  
	`<path> <args>: runs the program at <path> with arguments <args>
	jobs: lists stopped jobs and running background jobs
	bg <job>: change a stopped job to a running background job
	fg <job>: change a running background job to a foreground job`
