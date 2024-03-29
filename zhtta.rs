//
// zhtta.rs
//
// Starting code for PS3
// Running on Rust 0.9
//
// Note that this code has serious security risks!  You should not run it 
// on any system with access to sensitive files.
// 
// University of Virginia - cs4414 Spring 2014
// Weilin Xu and David Evans
// Version 0.5

// To see debug! outputs set the RUST_LOG environment variable, e.g.: export RUST_LOG="zhtta=debug"

#[feature(globs)];
extern mod extra;

use std::io::*;
use std::io::net::ip::{SocketAddr};
use std::{os, str, run, libc, from_str};
use std::path::Path;
use std::hashmap::HashMap;

use extra::getopts;
use extra::arc::MutexArc;
use extra::arc::RWArc;
use extra::sync::Semaphore;

use extra::time::{get_time, Timespec};
static SERVER_NAME : &'static str = "Zhtta Version 0.5";

static IP : &'static str = "127.0.0.1";
static PORT : uint = 4414;
static MAX_SIZE :u64 = 6000000;
static WWW_DIR : &'static str = "./www";

static HTTP_OK : &'static str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n";
static HTTP_BAD : &'static str = "HTTP/1.1 404 Not Found\r\n\r\n";

static COUNTER_STYLE : &'static str = "<doctype !html><html><head><title>Hello, Rust!</title>
             <style>body { background-color: #884414; color: #FFEEAA}
                    h1 { font-size:2cm; text-align: center; color: black; text-shadow: 0 0 4mm red }
                    h2 { font-size:2cm; text-align: center; color: black; text-shadow: 0 0 4mm green }
             </style></head>
             <body>";


struct Page {
    size: u64,
    data: ~[u8],
    accesses: int,
    last_access: Timespec,
}

impl Page {
    fn new(_size:u64, _data: ~[u8]) -> Page{
        Page {
            size: _size,
            data: _data,
            accesses: 0,
            last_access: get_time(),
        }
    }
    fn update(&mut self) {
        self.accesses = self.accesses+1;
        self.last_access = get_time();
    }
}
struct Cache {
    max_size: u64,
    current_size: u64,
    files: HashMap<~str, Page>,
}
impl Cache {
    fn new() -> Cache {
        Cache {
            max_size: MAX_SIZE,
            current_size: 0,
            files: HashMap::new(),
        }
    }
    //maybe only access if it exists, and then write it instead of trying to transfer the string
    //or maybe pop the Page, use it, and then reinsert it because of caching algorithm
//    #[ignore(dead_code)]
//    fn get_files(~self) -> HashMap<~str, Page> {
//        (self.files)
//    }
    fn get(&mut self, path:&~str, stream:  Option<std::io::net::tcp::TcpStream>) {
        debug!("Cache get {:?}", path);
        let p: &mut Page = self.files.get_mut(path);
        p.update();
        let mut stream = stream;
        stream.write(p.data) ;
    }
    fn contains(&self, req: &HTTP_Request)->bool {
        self.files.contains_key(&(req.path.filename_str().unwrap().to_owned()))
    }
    fn load(&mut self, _path:~Path, _data: ~[u8]){
        debug!("Cache load {:?}", _path.filename_str());
        let p = Page::new(_path.stat().size, _data);
        let psize =_path.stat().size;
        //as long as the file is less than the size of the cache
        if psize < self.max_size {
            //if the size of the cache would be over the limit
            if self.current_size + psize > self.max_size {
                debug!("Cache is full!");
                //remove oldest stuff until you have room
                while self.current_size + psize > self.max_size && !self.files.is_empty() {
                    self.remove_oldest();
                }
            }
            //insert file
            debug!("Inserting {:?} into cache", _path.filename_str());
            self.files.insert(_path.filename_str().unwrap().to_owned(), p);
        }
    }
    fn remove_oldest(&mut self) {
        if self.files.len() <= 0 {return}
        let mut o_key:~str = ~"";
        let mut o_val:Timespec= get_time();
        for (key, val) in self.files.iter() {
            if val.last_access < o_val {
                o_val = val.last_access.clone();
                o_key = key.clone();
            }
        }
        if o_key != ~"" {
            let v = self.files.pop(&o_key).unwrap();
            self.current_size = self.current_size - v.size;
        }
    }



}

//if cache.access().contains_key(request.path){
//    stream(cache.access().get(request.path));
//}
//else {
//    let f = normal_load_and_stream(request.path);
//    cache.load(request.path, f);
//}

struct HTTP_Request {
    // Use peer_name as the key to access TcpStream in hashmap. 

    // (Due to a bug in extra::arc in Rust 0.9, it is very inconvenient to use TcpStream without the "Freeze" bound.
    //  See issue: https://github.com/mozilla/rust/issues/12139)
    peer_name: ~str,
    path: ~Path,
}
impl HTTP_Request {
    fn clone(&self) -> HTTP_Request {
        HTTP_Request {
            peer_name:self.peer_name.clone(),
            path:self.path.clone(),
        }
    }
}

struct WebServer {
    ip: ~str,
    port: uint,
    www_dir_path: ~Path,
    
    visitor_arc: RWArc<uint>,

    request_queue_arc: MutexArc<~[HTTP_Request]>,
    stream_map_arc: MutexArc<HashMap<~str, Option<std::io::net::tcp::TcpStream>>>,
    
    notify_port: Port<()>,
    shared_notify_chan: SharedChan<()>,

    tasks: int,
}

impl WebServer {
    fn new(ip: &str, port: uint, www_dir: &str) -> WebServer {
        let (notify_port, shared_notify_chan) = SharedChan::new();
        let www_dir_path = ~Path::new(www_dir);
        os::change_dir(www_dir_path.clone());
        let num_str = WebServer::run_cmd_in_gash("nproc");
        let trimmed = num_str.trim();
        let num: int = match from_str::<int>(trimmed) {
            Some(i) if i > 0 => {debug!("found {:?} cores",i); i},
            Some(_) => 1,
            None=>1,
        };
        
        WebServer {
            ip: ip.to_owned(),
            port: port,
            www_dir_path: www_dir_path,     

    	    visitor_arc: RWArc::new(0u),
            request_queue_arc: MutexArc::new(~[]),
            stream_map_arc: MutexArc::new(HashMap::new()),
            
            notify_port: notify_port,
            shared_notify_chan: shared_notify_chan,        

            tasks:num.clone(),
        }
    }
    
    fn run(&mut self) {
        self.listen();
        self.dequeue_static_file_request();
    }
    
    fn listen(&mut self) {
        let addr = from_str::<SocketAddr>(format!("{:s}:{:u}", self.ip, self.port)).expect("Address error.");
        let www_dir_path_str = self.www_dir_path.as_str().expect("invalid www path?").to_owned();

        let request_queue_arc = self.request_queue_arc.clone();
        let shared_notify_chan = self.shared_notify_chan.clone();
        let stream_map_arc = self.stream_map_arc.clone();
        let visitor_arc = self.visitor_arc.clone();        

        spawn(proc() {
            let mut acceptor = net::tcp::TcpListener::bind(addr).listen();
            println!("{:s} listening on {:s} (serving from: {:s}).", 
                     SERVER_NAME, addr.to_str(), www_dir_path_str);

            for stream in acceptor.incoming() {
                let (queue_port, queue_chan) = Chan::new();
                queue_chan.send(request_queue_arc.clone());

                let notify_chan = shared_notify_chan.clone();
                let stream_map_arc = stream_map_arc.clone();

                // Spawn a task to handle the connection.
                let ccounter = visitor_arc.clone(); 
                spawn(proc() {
                    WebServer::update_count(ccounter.clone()); //finished safe counter

                    let request_queue_arc = queue_port.recv();
                    let mut stream = stream;

                    let peer_name = WebServer::get_peer_name(&mut stream);

                    let mut buf = [0, ..500];
                    stream.read(buf);
                    let request_str = str::from_utf8(buf);
                    debug!("Request:\n{:s}", request_str);

                    let req_group : ~[&str]= request_str.splitn(' ', 3).collect();
                    if req_group.len() > 2 {
                        let path_str = "." + req_group[1].to_owned();

                        let mut path_obj = ~os::getcwd();
                        path_obj.push(path_str.clone());

                        let ext_str = match path_obj.extension_str() {
                            Some(e) => e,
                            None => "",
                        };

                        debug!("Requested path: [{:s}]", path_obj.as_str().expect("error"));
                        debug!("Requested path: [{:s}]", path_str);

                        if path_str == ~"./" {
                            debug!("===== Counter Page request =====");
                            WebServer::respond_with_counter_page(stream, ccounter.clone()); //WebServer safe counter
                            debug!("=====Terminated connection from [{:s}].=====", peer_name);
                        } else if !path_obj.exists() || path_obj.is_dir() {
                            debug!("===== Error page request =====");
                            WebServer::respond_with_error_page(stream, path_obj);
                            debug!("=====Terminated connection from [{:s}].=====", peer_name);
                        } else if ext_str == "shtml" { // Dynamic web pages.
                            debug!("===== Dynamic Page request =====");
                            WebServer::respond_with_dynamic_page(stream, path_obj);
                            debug!("=====Terminated connection from [{:s}].=====", peer_name);
                        } else { 
                            debug!("===== Static Page request =====");
                            let path_clone = path_obj.clone();
                            let path_size = path_clone.stat().size;
                            if path_size < 512 {
                                WebServer::respond_with_small_file(stream, path_obj);
                                debug!("Handling small static file in listener");
                            } else {
                                WebServer::enqueue_static_file_request(stream, path_obj, stream_map_arc, request_queue_arc, notify_chan);
                            }
                        }
                    }
                });
            }
        });
    }
    //use RWArc to update static variable
    fn update_count(counter: RWArc<uint>) { 
        counter.write(|count|{*count+=1;})
    }

    fn respond_with_error_page(stream: Option<std::io::net::tcp::TcpStream>, path: &Path) {
        let mut stream = stream;
        let msg: ~str = format!("Cannot open: {:s}", path.as_str().expect("invalid path").to_owned());
        stream.write(HTTP_BAD.as_bytes());
        stream.write(msg.as_bytes());
    }

    // finished: Safe visitor counter.
    fn respond_with_counter_page(stream: Option<std::io::net::tcp::TcpStream>, counter: RWArc<uint>) {
        let mut stream = stream;
        debug!("Reading count");
        let count:uint = counter.read(|count|{(return *count)});
        debug!("Starting counter request");
        let response: ~str = 
            format!("{:s}{:s}<h1>Greetings, Krusty!</h1>
                     <h2>Visitor count: {:u}</h2></body></html>\r\n", 
                    HTTP_OK, COUNTER_STYLE, 
                    count );
        debug!("Responding to counter request");
        stream.write(response.as_bytes());
    }
    fn respond_with_small_file(stream: Option<std::io::net::tcp::TcpStream>, path: &Path) {
        let mut stream = stream;
        let mut file_reader = File::open(path).expect("Invalid file!");
        //Return an iterator that reads the bytes one by one until EoF
        let mut file_iter = file_reader.bytes();
        for b in file_iter {
            stream.write_u8(b);
        }
    }
    
    // FINISHED: Streaming file.
    // TODO: Application-layer file caching.
    fn respond_with_static_file(stream: Option<std::io::net::tcp::TcpStream>, http: &HTTP_Request, cache_get: &MutexArc<Cache>) {
        let mut stream = stream;
        let mut file_reader = File::open(http.path).expect("Invalid file!");
        //Return an iterator that reads the bytes one by one until EoF
        let mut file_iter = file_reader.bytes();
        let (stream_port, stream_chan) = Chan::new();
        //check to make sure the file is small enough to be cached
        if http.path.stat().size < MAX_SIZE {
            //if the cache contains the request, this will be received in the first access
            //otherwise it will be received in the second one
            stream_chan.send(stream);
            //ports for cache use
            let (bool_port, bool_chan) = Chan::new();
            //access cache to see if file is loaded
            cache_get.access(|cache| {
                if cache.contains(http) {
                    let stream = stream_port.recv();
                    cache.get(&(http.path.filename_str().unwrap().to_owned()), stream);
                    bool_chan.send(false);
                }
                else {
                    //release cache until we have all of the data
                    bool_chan.send(true);
                }
            });
            
            let flag = bool_port.recv();
            //if the cache doesn't contain the file, run this
            if flag {
                let mut stream = stream_port.recv();
                //write file and store it in memory at the same time
                let mut data  = ~[];
                for b in file_iter {
                    data.push(b);
                    stream.write_u8(b);
                }
                //access cache and load data
                cache_get.access(|cache| {
                    //check again just to be sure
                    if !cache.contains(http) {
                        cache.load(((http.path).clone()), data.clone());
                    }
                });
            }
        }
        //if the file is too large, just stream it
        else {
            for b in file_iter {
                stream.write_u8(b);
            }
        }
    }

        // finished: Server-side gashing.
        // Testing with localhost:4414/index.shtml
    fn respond_with_dynamic_page(stream: Option<std::io::net::tcp::TcpStream>, path: &Path) {
        // for now, just serve as static file
    	let mut stream = stream;
        let mut file_reader = File::open(path).expect("Invalid file!");
        let file_contents = file_reader.read_to_str();
        let cmd_start = file_contents.find_str("<!--#exec cmd=").unwrap(); //find start of command
        let cmd_end = file_contents.find_str("-->").unwrap() + 3; //find end of command
        let cmd = file_contents.slice(cmd_start, cmd_end); //slice the command out
        let split_cmd: ~[&str] = cmd.split('"').collect(); //parse command for the gash command
        let gash_output: ~str = WebServer::run_cmd_in_gash(split_cmd[1]);
        let response: ~str =
            format!("{}{}{}", file_contents.slice_to(cmd_start), gash_output, file_contents.slice_from(cmd_end));	
        stream.write(response.as_bytes());
    }
        //Run gash and run the command sent to it and return the output	
    fn run_cmd_in_gash(cmd: &str) -> ~str {
        let mut gash = run::Process::new("./gash", &[~"-c",cmd.to_owned()], run::ProcessOptions::new()).unwrap();
        debug!("gash instance: {:?}", gash);
        let s = gash.output().read_to_str();
        debug!("gash output {:s}", s);
        gash.finish();
        debug!("done");
        return s;
    }
    
    // TODO: Smarter Scheduling.
    // Finished: Wahoo-first scheduling
    fn enqueue_static_file_request(stream: Option<std::io::net::tcp::TcpStream>, path_obj: &Path, stream_map_arc: MutexArc<HashMap<~str, Option<std::io::net::tcp::TcpStream>>>, req_queue_arc: MutexArc<~[HTTP_Request]>, notify_chan: SharedChan<()>) {
        // Save stream in hashmap for later response.
        let mut stream = stream;
        let peer_name = WebServer::get_peer_name(&mut stream);
        let (stream_port, stream_chan) = Chan::new();
        stream_chan.send(stream);
        unsafe {
            // Use an unsafe method, because TcpStream in Rust 0.9 doesn't have "Freeze" bound.
            stream_map_arc.unsafe_access(|local_stream_map| {
                let stream = stream_port.recv();
                local_stream_map.swap(peer_name.clone(), stream);
            });
        }
        
        // Enqueue the HTTP request.
        let req = HTTP_Request { peer_name: peer_name.clone(), path: ~path_obj.clone() };
        let (req_port, req_chan) = Chan::new();
        req_chan.send(req);

        debug!("Waiting for queue mutex lock.");
        req_queue_arc.access(|local_req_queue| {
            debug!("Got queue mutex lock.");
            let req: HTTP_Request = req_port.recv();
            if local_req_queue.len() == 0 {
                local_req_queue.push(req);
            } 
            else {
                let req_ip = req.peer_name.clone();
                let sub_1 = req_ip.slice(0, 8).to_owned();
                let sub_2 = req_ip.slice(0, 7).to_owned();

                for i in range(0, local_req_queue.len() - 1) {
                    let comp_ip = local_req_queue[i].peer_name.clone();
                    let comp_1 = comp_ip.slice(0, 8).to_owned();
                    let comp_2 = comp_ip.slice(0, 7).to_owned();
                    if (str::eq(&sub_1, &~"128.143.") || str::eq(&sub_2, &~"137.54.")) && !(str::eq(&comp_1, &~"128.143.") || str::eq(&comp_2, &~"137.54.")) {
                        local_req_queue.insert(i, req);
                        break;
                    } else if ((str::eq(&sub_1, &~"128.143.") || str::eq(&sub_2, &~"137.54.")) && (str::eq(&comp_1, &~"128.143.") || str::eq(&comp_2, &~"137.54."))) || (!(str::eq(&sub_1, &~"128.143.") || str::eq(&sub_2, &~"137.54.")) && !(str::eq(&comp_1, &~"128.143.") || str::eq(&comp_2, &~"137.54."))) {
                        let comp_path = req.path.clone();
                        let comp_size = comp_path.stat().size;
                        let req_path = req.path.clone();
                        let req_size = req_path.stat().size;
                        if req_size < comp_size {
                            local_req_queue.insert(i, req);
                            break;
                        }
                    } else if i == (local_req_queue.len() - 1) {
                        local_req_queue.push(req);
                        break;
                    }
                }
            }
 
        


        debug!("A new request enqueued, now the length of queue is {:u}.", local_req_queue.len());
        });
        
        notify_chan.send(()); // Send incoming notification to responder task.
    
    
    }
     // TODO: Smarter Scheduling.
    fn dequeue_static_file_request(&mut self) {
        let req_queue_get = self.request_queue_arc.clone();
        let stream_map_get = self.stream_map_arc.clone();
        let sem = Semaphore::new(1);     
        // Port<> cannot be sent to another task. So we have to make this task as the main task that can access self.notify_port.
        
        let (request_port, request_chan) = Chan::new();
        let port = MutexArc::new(request_port);

        let cache = Cache::new();
        let cache_get = MutexArc::new(cache);
        
        //Will allow for creating the same number of tasks as we have cores
        //Allows for the downloading of multiple different files at the same time
        //If it is the same file, it won't work
        debug!("Creating {:?} tasks", self.tasks);
        for i in range(0, self.tasks) {
            //request_port = request_port.clone();
            let stream_map_get = stream_map_get.clone();
            //let request_port = request_port.clone();
            let sem = sem.clone();
            let port = port.clone();
            let name = i.clone();
            let cache_get = cache_get.clone();
            debug!("Starting task {:?}", name)
            spawn( proc() {
                let name = name.clone();
                loop {
                    unsafe{
                    debug!("Task {:?} waiting", name)
                    sem.acquire();
                    debug!("Task {:?} get", name)
                    //let request: HTTP_Request = request_port.recv();
                    let request: HTTP_Request = port.unsafe_access(|req| {let r: HTTP_Request =(*req).recv(); r.clone()});
                    sem.release();
                    debug!("Task {:?} release", name);
                    // Get stream from hashmap.
                    // Use unsafe method, because TcpStream in Rust 0.9 doesn't have "Freeze" bound.
                    let (stream_port, stream_chan) = Chan::new();
                    stream_map_get.unsafe_access(|local_stream_map| {
                        let stream = local_stream_map.pop(&request.peer_name).expect("no option tcpstream");
                        stream_chan.send(stream);
                    });
                    
                    let stream = stream_port.recv();
                    WebServer::respond_with_static_file(stream, &request, &cache_get);
                    // Close stream automatically.
                    debug!("=====Terminated connection from [{:s}].=====", request.peer_name);
                    }
                }
            }
            );
        }
        let mut total = 0;
        loop {
            self.notify_port.recv();    // waiting for new request enqueued.
            total = total + 1;
            
            req_queue_get.access( |req_queue| {
                match req_queue.shift_opt() { // FIFO queue.
                    None => { /* do nothing */ }
                    Some(req) => {
                        request_chan.send(req);
                        debug!("A new request dequeued, now the length of queue is {:u}, {:?} served.", req_queue.len(), total);
                    }
                }
            });
        }
    
    }
    
    fn get_peer_name(stream: &mut Option<std::io::net::tcp::TcpStream>) -> ~str {
        match *stream {
            Some(ref mut s) => {
                         match s.peer_name() {
                            Some(pn) => {pn.to_str()},
                            None => (~"")
                         }
                       },
            None => (~"")
        }
    }
}

fn get_args() -> (~str, uint, ~str) {
    fn print_usage(program: &str) {
        println!("Usage: {:s} [options]", program);
        println!("--ip     \tIP address, \"{:s}\" by default.", IP);
        println!("--port   \tport number, \"{:u}\" by default.", PORT);
        println!("--www    \tworking directory, \"{:s}\" by default", WWW_DIR);
        println("-h --help \tUsage");
    }
    
    /* Begin processing program arguments and initiate the parameters. */
    let args = os::args();
    let program = args[0].clone();
    
    let opts = ~[
        getopts::optopt("ip"),
        getopts::optopt("port"),
        getopts::optopt("www"),
        getopts::optflag("h"),
        getopts::optflag("help")
    ];

    let matches = match getopts::getopts(args.tail(), opts) {
        Ok(m) => { m }
        Err(f) => { fail!(f.to_err_msg()) }
    };

    if matches.opt_present("h") || matches.opt_present("help") {
        print_usage(program);
        unsafe { libc::exit(1); }
    }
    
    let ip_str = if matches.opt_present("ip") {
                    matches.opt_str("ip").expect("invalid ip address?").to_owned()
                 } else {
                    IP.to_owned()
                 };
    
    let port:uint = if matches.opt_present("port") {
                        from_str::from_str(matches.opt_str("port").expect("invalid port number?")).expect("not uint?")
                    } else {
                        PORT
                    };
    
    let www_dir_str = if matches.opt_present("www") {
                        matches.opt_str("www").expect("invalid www argument?") 
                      } else { WWW_DIR.to_owned() };
    
    (ip_str, port, www_dir_str)
}

fn main() {
    let (ip_str, port, www_dir_str) = get_args();
    let mut zhtta = WebServer::new(ip_str, port, www_dir_str);
    zhtta.run();
}

    
