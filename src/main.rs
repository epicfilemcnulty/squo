use actix_web::{get, web, App, HttpServer};
use std::fs;
use std::io;
use std::io::{BufRead, BufReader, Error, ErrorKind, Result};

enum MetricType {
    Counter,
    Gauge,
    Untyped,
}

impl MetricType {
    fn display(&self) -> &str {
        match *self {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Untyped => "untyped",
        }
    }
}

struct Metric<'a> {
    name: &'a str,
    ptype: MetricType,
    values: Vec<String>,
}

impl Metric<'_> {
    fn add(&mut self, value: &str, labels: Option<Vec<(&str, &str)>>) {
        let mut output = String::from(self.name);
        if labels.is_some() {
            output.push_str("{");
            for label in labels.unwrap() {
                output.push_str(&format!("{}=\"{}\",", label.0, label.1));
            }
            output.pop(); // remove the last comma
            output.push_str("}");
        }
        output.push_str(&format!(" {}", value));
        self.values.push(output);
    }

    fn render(&self) -> String {
        format!(
            "# TYPE {} {}\n{}\n",
            self.name,
            self.ptype.display(),
            self.values.join("\n")
        )
    }
}

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
    let mt: Vec<&str> = mem_stats[0].split_whitespace().collect();
    let mf: Vec<&str> = mem_stats[1].split_whitespace().collect();
    let ma: Vec<&str> = mem_stats[2].split_whitespace().collect();

    let mut mem_total = Metric {
        name: "squo_mem_total",
        ptype: MetricType::Gauge,
        values: Vec::new(),
    };
    mem_total.add(mt[1], None);
    let mut mem_free = Metric {
        name: "squo_mem_free",
        ptype: MetricType::Gauge,
        values: Vec::new(),
    };
    mem_free.add(mf[1], None);
    let mut mem_available = Metric {
        name: "squo_mem_available",
        ptype: MetricType::Gauge,
        values: Vec::new(),
    };
    mem_available.add(ma[1], None);

    Ok(format!(
        "{}{}{}",
        mem_total.render(),
        mem_free.render(),
        mem_available.render()
    ))
}

fn get_network_info() -> Result<String> {
    /*
    Inter-|   Receive                                                |  Transmit
     face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
       lo:| 1598190   16221    0    0    0     0          0         0  1598190   16221    0    0    0     0       0          0

    */
    let network_stats = file_to_vec("/proc/net/dev")?;
    let mut bytes_sent = Metric {
        name: "squo_network_bytes_sent",
        ptype: MetricType::Counter,
        values: Vec::new(),
    };
    let mut bytes_received = Metric {
        name: "squo_network_bytes_received",
        ptype: MetricType::Counter,
        values: Vec::new(),
    };
    for iface in network_stats {
        if !iface.contains("|") {
            let stats: Vec<&str> = iface.split_whitespace().collect();
            let mut device = stats[0].to_string();
            device.pop(); // remove the last colon from interface name
            bytes_sent.add(&format!("{}", stats[9]), Some([("device", device.as_str())].to_vec()));
            bytes_received.add(&format!("{}", stats[1]), Some([("device", device.as_str())].to_vec()));
        }
    }
    Ok(format!("{}{}", bytes_received.render(), bytes_sent.render()))
}

fn get_disk_info(disk_mounts: &str) -> Result<String> {
    let mut disk_total = Metric {
        name: "squo_disk_total",
        ptype: MetricType::Gauge,
        values: Vec::new(),
    };
    let mut disk_free = Metric {
        name: "squo_disk_free",
        ptype: MetricType::Gauge,
        values: Vec::new(),
    };
    let mounts: Vec<&str> = disk_mounts.split_whitespace().collect();
    for mount in mounts {
        let fs = nix::sys::statvfs::statvfs(mount).map_err(|_| Error::new(ErrorKind::Other, "statfs"))?;
        let bs = fs.block_size();
        let bl_total = fs.blocks() * bs;
        let bl_avail = fs.blocks_available() * bs;
        disk_total.add(&format!("{}", bl_total), Some([("path", mount)].to_vec()));
        disk_free.add(&format!("{}", bl_avail), Some([("path", mount)].to_vec()));
    }
    Ok(format!("{}{}", disk_total.render(), disk_free.render()))
}

fn get_cpu_info() -> Result<String> {
    let stats = nix::sys::sysinfo::sysinfo().map_err(|_| Error::new(ErrorKind::Other, "sysinfo"))?;
    let mut la_1m = Metric {
        name: "squo_load_avg_1m",
        ptype: MetricType::Gauge,
        values: Vec::new(),
    };
    let cpus = num_cpus::get();
    let (la, _, _) = stats.load_average();
    la_1m.add(&format!("{:.3}", la / cpus as f64), None);
    Ok(format!("{}", la_1m.render()))
}

#[get("/metrics")]
async fn metrics(data: web::Data<State>) -> Result<String> {
    Ok(format!(
        "{}{}{}{}",
        get_cpu_info()?,
        get_mem_info()?,
        get_disk_info(&data.disk_mounts)?,
        get_network_info()?,
    ))
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
