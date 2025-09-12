<img src="img/logo.svg" alt="logo" width="200"/>

# Bing Maps 3D Tile Downloader


Rust CLI tool for downloading and decompressing Bing Maps 3D tiles (GLB format) with parallel processing capabilities

## Features

- **Download 3D Tiles**: Fetch Bing Maps 3D tiles as GLB files for specified geographic regions
- **Parallel Processing**: High-performance concurrent downloading with configurable concurrency limits
- **Area Selection**: Define download areas by bounding box coordinates or center point with size
- **Texture Decompression**: Decompress KTX2 textures in GLB files using gltf-transform

## Installation

### Prerequisites

- Rust 1.70+ (uses 2024 edition)
- For decompression: `gltf-transform` CLI tool or Node.js with npx

### Build from Source

```bash
git clone https://github.com/s1dny/bing_maps_tile_downloader
cd bing_maps_tile_downloader
cargo build --release
```

The compiled binary will be available at `target/release/bing`

## Usage

### Download 3D Tiles

#### By Center Point and Size
```bash
# Download a 500m × 500m area around Sydney Opera House
./bing download --center-coord="-33.865143,151.209900" --size=500 --zoom=18 --out=./sydney_tiles
```

#### By Bounding Box
```bash
# Download tiles within specified SW and NE coordinates
./bing download --sw-coord="-33.870000,151.200000" --ne-coord="-33.860000,151.220000" --zoom=18
```

#### Download Options
-   `--center-coord <LAT,LON>`: Center of the area to download (e.g., "-33.86,151.20")
-   `--size <METERS>`: The side length of a square area to download, in meters
-   `--sw-coord <LAT,LON>`: South-west corner of a bounding box
-   `--ne-coord <LAT,LON>`: North-east corner of a bounding box
-   `--zoom <LEVEL>`: Zoom level for the tiles (default: 18)
-   `--out <DIR>`: Output directory for tiles (default: `./tiles`)
-   `--api-key <KEY>`: Bing Maps API key
-   `--concurrency <NUM>`: Number of concurrent download requests (default: 100)

### Decompress Textures

```bash
# Decompress KTX2 textures in all GLB files in current directory
./bing decompress

# Process specific directory with custom output
./bing decompress ./tiles --out ./processed_tiles --recursive

# Use more worker threads
./bing decompress --jobs 8 --force --recursive
```

#### Decompression Options
-   `[INPUT_DIR]`: Directory to scan for `.glb` files (default: current directory).
-   `--out <DIR>`: Output directory for processed files (default: `<INPUT_DIR>/processed`).

## Examples

### Basic Workflow

1. **Download tiles for a landmark**:
   ```bash
   ./bing download --center-coord="40.748817,-73.985428" --size=1000 --zoom=19 --out=./empire_state
   ```

2. **Decompress the downloaded tiles**:
   ```bash
   ./bing decompress ./empire_state --out ./empire_state_processed --recursive
   ```