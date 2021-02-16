use actix_web::{get, web, App, HttpServer};
use std::fs;
use std::io;
use std::io::{BufRead, BufReader, Error, ErrorKind, Result};

fn file_to_vec(filename: &str) -> Result<Vec<String>> {
    let file_in = fs::File::open(filename)?;
    let file_reader = BufReader::new(file_in);
    Ok(file_reader.lines().filter_map(io::Result::ok).collect())
}

fn get_mem_info() -> Result<String> {
    /*
    MemTotal:       32149948 kB
    MemFree:        17528172 kB
    MemAvailable:   25903096 kB
    Buffers:          580648 kB
    Cached:          7887768 kB
    SwapCached:            0 kB
    Active:          6931744 kB
    Inactive:        5244988 kB
    Active(anon):    3679092 kB
    Inactive(anon):   182128 kB
    Active(file):    3252652 kB
    Inactive(file):  5062860 kB
    Unevictable:          80 kB
    Mlocked:              80 kB
    SwapTotal:       2097148 kB
    SwapFree:        2097148 kB
    Dirty:               324 kB
    Writeback:             0 kB
    AnonPages:       3708632 kB
    Mapped:           878520 kB
    Shmem:            185512 kB
    KReclaimable:     519400 kB
    */
    let mem_stats = file_to_vec("/proc/meminfo")?;
    let mem_total: Vec<&str> = mem_stats[0].split_whitespace().collect();
    let mem_free: Vec<&str> = mem_stats[1].split_whitespace().collect();
    let mem_available: Vec<&str> = mem_stats[2].split_whitespace().collect();
    Ok(format!(
        "# TYPE squo_mem_total gauge\nsquo_mem_total {}\n# TYPE squo_mem_free gauge\nsquo_mem_free {}\n# TYPE squo_mem_available gauge\nsquo_mem_available {}\n",
        mem_total[1], mem_free[1], mem_available[1]
    ))
}

fn get_disk_info(disk_mounts: &str) -> Result<String> {
    let mounts: Vec<&str> = disk_mounts.split_whitespace().collect();
    let mut output = String::new();
    for mount in mounts {
        let fs = nix::sys::statvfs::statvfs(mount).map_err(|_| Error::new(ErrorKind::Other, "statfs"))?;
        let bs = fs.block_size();
        let bl_total = fs.blocks();
        let bl_avail = fs.blocks_available();
        output.push_str(&format!(
            "# TYPE squo_disk_total gauge\nsquo_disk_total{{path=\"{}\"}} {}\n# TYPE squo_disk_free gauge\nsquo_disk_free{{path=\"{}\"}} {}\n",
            mount, bl_total*bs, mount, bl_avail*bs
        ));
    }
    Ok(output)
}

fn get_node_stats(disk_mounts: &str) -> Result<String> {
    let stats = nix::sys::sysinfo::sysinfo().map_err(|_| Error::new(ErrorKind::Other, "sysinfo"))?;
    let cpus = num_cpus::get();
    let (la, _, _) = stats.load_average();
    let la = la / cpus as f64;
    Ok(format!(
        "# TYPE squo_load_average_1m gauge\nsquo_load_average_1m {:.3}\n{}{}",
        la,
        get_mem_info()?,
        get_disk_info(disk_mounts)?,
    ))
}

#[get("/metrics")]
async fn metrics(data: web::Data<State>) -> Result<String> {
    Ok(get_node_stats(&data.disk_mounts)?)
}

struct State {
    disk_mounts: String,
}

#[actix_web::main]
async fn main() -> Result<()> {
    HttpServer::new(|| {
        App::new()
            .data(State {
                disk_mounts: std::env::var("SQUO_DISK_MOUNTS").unwrap_or_else(|_| String::from("/")),
            })
            .service(metrics)
    })
    .bind("0.0.0.0:9100")?
    .run()
    .await
}
