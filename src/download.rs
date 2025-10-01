use anyhow::{anyhow, Context, Result};
use clap::Parser;
use futures::stream::{self, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use std::cmp::{max, min};
use std::f64::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs as tokio_fs;

const EARTH_LAT_MAX: f64 = 85.05112878;
const DEFAULT_HOST: &str = "https://t.ssl.ak.tiles.virtualearth.net";
const DEFAULT_G: &str = "15340";
const DEFAULT_TF: &str = "3dv4";
const USER_AGENT: &str = "TileFetcher/1.0 (+https://example.local)";

#[derive(Parser, Debug)]
pub struct Args {
    /// SW corner (lat,lon)
    #[arg(long = "sw-coord")]
    pub sw_coord: Option<String>,

    /// NE corner (lat,lon)
    #[arg(long = "ne-coord")]
    pub ne_coord: Option<String>,

    /// Center (lat,lon)
    #[arg(long = "center-coord")]
    pub center_coord: Option<String>,

    /// Square size in meters
    #[arg(long = "size")]
    pub size: Option<f64>,

    /// Output directory
    #[arg(long = "out", default_value = "./tiles")]
    pub out: PathBuf,

    /// Bing API key
    #[arg(long = "api-key", default_value = "Ar9wCt_eD79MwUsC3wup-erRDfnN0VKqPSZQ4yiCNDucBOJBeflFCNZQUgocler6")]
    pub api_key: String,

    /// Zoom level (max ~20)
    #[arg(long = "zoom", default_value_t = 18)]
    pub zoom: u32,

    /// Concurrent requests
    #[arg(long = "concurrency", default_value_t = 100)]
    pub concurrency: usize,

    /// Split tiles into a grid of subdirectories (must be a perfect square: 1, 4, 9, 16, 25, etc.)
    #[arg(long = "split", default_value_t = 1)]
    pub split: usize,
}

#[inline]
fn meters_to_degrees(meters: f64, lat_deg: f64) -> (f64, f64) {
    // Spherical approximations consistent with Web Mercator usage.
    let r = 6_371_000.0_f64;
    let lat = meters / (r * PI / 180.0);
    let lon = meters / (r * (lat_deg.to_radians().cos()) * PI / 180.0);
    (lat, lon)
}

#[inline]
fn create_square_bbox(center_lat: f64, center_lon: f64, size_m: f64) -> (f64, f64, f64, f64) {
    let half = size_m / 2.0;
    let (dlat, dlon) = meters_to_degrees(half, center_lat);
    (
        center_lat - dlat,
        center_lon - dlon,
        center_lat + dlat,
        center_lon + dlon,
    )
}

#[inline]
fn clamp_lat(lat: f64) -> f64 {
    lat.max(-EARTH_LAT_MAX).min(EARTH_LAT_MAX)
}

#[inline]
fn wrap_lon(lon: f64) -> f64 {
    // Safe wrap into [-180, 180)
    let mut l = lon % 360.0;
    if l >= 180.0 {
        l -= 360.0;
    }
    if l < -180.0 {
        l += 360.0;
    }
    l
}

#[inline]
fn lonlat_to_tile_xy(lon: f64, lat: f64, z: u32) -> (i32, i32) {
    let lat = clamp_lat(lat);
    let lon = wrap_lon(lon);
    let n = (1u32 << z) as f64;
    let lat_rad = lat.to_radians();

    let xf = ((lon + 180.0) / 360.0) * n;
    let yf = (0.5 - ( ( (PI / 4.0) + (lat_rad / 2.0) ).tan().ln() / (2.0 * PI) )) * n;

    // Python's int() floors for positive values; ensure we floor.
    (xf.floor() as i32, yf.floor() as i32)
}

#[inline]
fn tile_xy_to_quadkey(x: i32, y: i32, z: u32) -> String {
    let mut q = String::with_capacity(z as usize);
    let x_temp = x;
    let y_temp = y;
    for i in (1..=z).rev() {
        let mask = 1 << (i - 1);
        let mut digit = 0;
        if (x_temp & mask) != 0 { digit += 1; }
        if (y_temp & mask) != 0 { digit += 2; }
        q.push(char::from(b'0' + digit));
    }
    q
}

fn bbox_tile_ranges(lat1: f64, lon1: f64, lat2: f64, lon2: f64, z: u32) -> Vec<(i32, i32, i32, i32)> {
    let a_lon = wrap_lon(lon1);
    let b_lon = wrap_lon(lon2);
    let a_lat = clamp_lat(lat1);
    let b_lat = clamp_lat(lat2);

    let (lon_min, lon_max) = if a_lon <= b_lon { (a_lon, b_lon) } else { (b_lon, a_lon) };
    let (lat_min, lat_max) = if a_lat <= b_lat { (a_lat, b_lat) } else { (b_lat, a_lat) };

    let crosses_am = a_lon > b_lon;

    let y1 = lonlat_to_tile_xy(lon_min, lat_min, z).1;
    let y2 = lonlat_to_tile_xy(lon_min, lat_max, z).1;
    let y3 = lonlat_to_tile_xy(lon_max, lat_min, z).1;
    let y4 = lonlat_to_tile_xy(lon_max, lat_max, z).1;
    let y_min = min(min(y1, y2), min(y3, y4));
    let y_max = max(max(y1, y2), max(y3, y4));

    if !crosses_am {
        let x_min = lonlat_to_tile_xy(lon_min, lat_min, z).0;
        let x_max = lonlat_to_tile_xy(lon_max, lat_min, z).0;
        vec![(x_min, x_max, y_min, y_max)]
    } else {
        let x_min_a = lonlat_to_tile_xy(lon_min, lat_min, z).0;
        let x_max_a = ((1i32 << z) - 1) as i32;
        let x_min_b = 0i32;
        let x_max_b = lonlat_to_tile_xy(lon_max, lat_min, z).0;
        vec![(x_min_a, x_max_a, y_min, y_max), (x_min_b, x_max_b, y_min, y_max)]
    }
}

fn iter_tiles_in_ranges(ranges: &[(i32, i32, i32, i32)]) -> Vec<(i32, i32)> {
    let mut tiles = Vec::new();
    for &(x_min, x_max, y_min, y_max) in ranges {
        for y in y_min..=y_max {
            for x in x_min..=x_max {
                tiles.push((x, y));
            }
        }
    }
    tiles
}

async fn download_one(client: &reqwest::Client, url: &str, out_path: &Path) -> Result<bool> {
    if let Some(parent) = out_path.parent() {
        tokio_fs::create_dir_all(parent).await.ok();
    }

    let resp = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("GET {}", url))?;

    if !resp.status().is_success() {
        eprintln!("HTTP {} for {}", resp.status(), url);
        return Ok(false);
    }

    let bytes = resp.bytes().await?;
    if bytes.is_empty() {
        eprintln!("Empty response for {}", url);
        return Ok(false);
    }

    let tmp_path = out_path.with_extension(format!(
        "{}.part",
        out_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
    ));

    tokio_fs::write(&tmp_path, &bytes).await?;
    // atomic-ish move
    fs::rename(&tmp_path, out_path).with_context(|| "rename .part → final")?;
    Ok(true)
}

fn parse_coordinates(s: &str) -> Result<(f64, f64)> {
    let parts: Vec<_> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() != 2 {
        return Err(anyhow!("Coordinates must be in format 'latitude,longitude'"));
    }
    let lat: f64 = parts[0].parse()?;
    let lon: f64 = parts[1].parse()?;
    Ok((lat, lon))
}

fn validate_and_get_grid_size(split: usize) -> Result<usize> {
    if split == 0 {
        return Err(anyhow!("Split parameter must be greater than 0"));
    }
    
    let grid_size = (split as f64).sqrt() as usize;
    if grid_size * grid_size != split {
        return Err(anyhow!("Split parameter must be a perfect square (1, 4, 9, 16, 25, etc.), got {}", split));
    }
    
    Ok(grid_size)
}

fn get_tile_subfolder(x: i32, y: i32, grid_size: usize) -> String {
    if grid_size == 1 {
        return String::new();
    }
    
    let grid_x = (x.abs() as usize) % grid_size;
    let grid_y = (y.abs() as usize) % grid_size;
    
    format!("{:02}_{:02}", grid_x, grid_y)
}

pub async fn run_download(args: Args) -> Result<()> {

    // Validate split parameter
    let grid_size = validate_and_get_grid_size(args.split)?;

    // Determine bbox
    let (lat1, lon1, lat2, lon2) = if let (Some(center), Some(size)) = (&args.center_coord, args.size)
    {
        let (clat, clon) = parse_coordinates(center)?;
        let (a, b, c, d) = create_square_bbox(clat, clon, size);
        println!(
            "Calculated bbox: ({:.6}, {:.6}) to ({:.6}, {:.6})",
            a, b, c, d
        );
        (a, b, c, d)
    } else if let (Some(sw), Some(ne)) = (&args.sw_coord, &args.ne_coord) {
        let (lat_sw, lon_sw) = parse_coordinates(sw)?;
        let (lat_ne, lon_ne) = parse_coordinates(ne)?;
        println!(
            "Using specified bbox: ({:.6}, {:.6}) to ({:.6}, {:.6})",
            lat_sw, lon_sw, lat_ne, lon_ne
        );
        (lat_sw, lon_sw, lat_ne, lon_ne)
    } else {
        eprintln!("ERROR: Must specify either (--sw-coord, --ne-coord) OR (--center-coord, --size)");
        return Ok(());
    };

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(32)
        .build()?;

    let z = args.zoom;

    let ranges = bbox_tile_ranges(lat1, lon1, lat2, lon2, z);
    let tiles = iter_tiles_in_ranges(&ranges);
    if tiles.is_empty() {
        println!("No tiles in the specified range.");
        return Ok(());
    }
    
    println!("Zoom level: {}", args.zoom);
    println!("Tile range: {:?}", ranges);
    println!("Tile total: {} ", tiles.len());
    println!("Concurrency: {}", args.concurrency);
    if args.split > 1 {
        println!("Split: {} ({}x{} grid)", args.split, grid_size, grid_size);
    }
    println!("Directory: {}", args.out.display());

    let pb = ProgressBar::new(tiles.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:30.cyan/blue}] {pos}/{len} - ETA {eta}",
        )
        .unwrap(),
    );

    let ok_count = Arc::new(AtomicUsize::new(0));
    let ok_count_clone = ok_count.clone();
    let out_dir = Arc::new(args.out);
    let api_key = Arc::new(args.api_key);
    let client = Arc::new(client);
    let host = Arc::new(DEFAULT_HOST.to_string());
    let grid_size = Arc::new(grid_size);

    // Work stream with bounded concurrency, progress updates as each completes.
    stream::iter(tiles.into_iter())
        .for_each_concurrent(args.concurrency, {
            let pb = pb.clone();
            move |(x, y)| {
                let pb = pb.clone();
                let ok_count = ok_count_clone.clone();
                let out_dir = out_dir.clone();
                let api_key = api_key.clone();
                let client = client.clone();
                let host = host.clone();
                let grid_size = grid_size.clone();

                async move {
                    let qk = tile_xy_to_quadkey(x, y, z);
                    let url = format!(
                        "{}/tiles/mtx{}?g={}&tf={}&n=z&key={}&form=web3d",
                        host, qk, DEFAULT_G, DEFAULT_TF, api_key
                    );
                    
                    // Determine subfolder based on tile coordinates
                    let subfolder = get_tile_subfolder(x, y, *grid_size);
                    let final_dir = if subfolder.is_empty() {
                        out_dir.as_ref().clone()
                    } else {
                        out_dir.join(&subfolder)
                    };
                    
                    let out_path = final_dir.join(format!("{}_{}_{}.glb", z, x, y));
                    let res = download_one(&client, &url, &out_path).await.unwrap_or_else(|e| {
                        eprintln!("Exception downloading {}: {}", url, e);
                        false
                    });
                    if res {
                        ok_count.fetch_add(1, Ordering::Relaxed);
                    }
                    pb.inc(1);
                }
            }
        })
        .await;

    pb.finish_and_clear();
    let ok = ok_count.load(Ordering::Relaxed);
    println!("Done: Saved {}/{} tiles", ok, ok);

    Ok(())
}
