<img src="img/logo.svg" alt="logo" width="200"/>

# Bing Maps 3D Tile Downloader


Rust CLI tool for downloading and decompressing Bing Maps 3D tiles (GLB format) with parallel processing capabilities

## Features

- **Download 3D Tiles**: Fetch Bing Maps 3D tiles as GLB files for specified geographic regions
- **Parallel Processing**: High-performance concurrent downloading with configurable concurrency limits
- **Area Selection**: Define download areas by bounding box coordinates or center point with size
- **Texture Decompression**: Decompress KTX2 textures in GLB files using gltf-transform
- **Directory Organization**: Split large tile collections into organized subdirectory grids for better file management

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

The compiled binary will be available at `target/release/bing`. You can add this to your path.

## Usage

### Download 3D Tiles

#### By Center Point and Size
```bash
# Download a 500m × 500m area around Sydney Opera House
cargo run --release download --center-coord="-33.856785245449856,151.21523854778107" --size=500 --zoom=18 --out=./sydney_tiles
```

#### By Bounding Box
```bash
# Download tiles within specified SW and NE coordinates
cargo run --release download --sw-coord="-33.870000,151.200000" --ne-coord="-33.860000,151.220000" --zoom=18
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
-   `--split <NUM>`: Split tiles into a grid of subdirectories (must be a perfect square: 1, 4, 9, 16, 25, etc.)

### Decompress Textures

```bash
# Decompress KTX2 textures in all GLB files in current directory
cargo run --release decompress

# Process specific directory with custom output
cargo run --release decompress ./tiles --out ./processed_tiles --recursive

# Use more worker threads
cargo run --release decompress --jobs 8 --force --recursive
```

#### Decompression Options
-   `[INPUT_DIR]`: Directory to scan for `.glb` files (default: current directory).
-   `--out <DIR>`: Output directory for processed files (default: `<INPUT_DIR>/processed`).
-   `--recursive`: Recurse into subdirectories
-   `--force`: Overwrite outputs if they already exist
-   `--jobs <NUM>`: Limit worker threads (default: number of logical CPUs)
-   `--use-npx`: Force using npx instead of globally installed gltf-transform
-   `--dry-run`: List what would be processed without executing
-   `--split <NUM>`: Split tiles into a grid of subdirectories (must be a perfect square: 1, 4, 9, 16, 25, etc.)

### Directory Organization with --split

The `--split` parameter helps organize large tile collections by distributing files across subdirectories in a grid pattern:

- `--split 4`: Creates a 2×2 grid (`00_00/`, `00_01/`, `01_00/`, `01_01/`)
- `--split 9`: Creates a 3×3 grid (`00_00/`, `00_01/`, `00_02/`, `01_00/`, etc.)
- `--split 16`: Creates a 4×4 grid of subdirectories

Files are distributed based on their tile coordinates, ensuring even distribution across subdirectories. The decompress command maintains this directory structure when processing files.

## Examples

### Basic Workflow

1. **Download tiles for a landmark**:
   ```bash
   cargo run --release download --center-coord="40.748817,-73.985428" --size=1000 --zoom=19 --out=./empire_state
   ```

2. **Decompress the downloaded tiles**:
   ```bash
   cargo run --release decompress ./empire_state --out ./empire_state_processed --recursive
   ```

### Advanced Examples

#### Large Area Download with Directory Organization
```bash
# Download a large area and organize into a 3x3 grid of subdirectories
cargo run --release download --center-coord="40.748817,-73.985428" --size=2000 --zoom=19 --split=9 --out=./manhattan_tiles
```

#### Processing Split Directory Structure
```bash
# Decompress tiles while maintaining the 3x3 grid organization
cargo run --release decompress ./manhattan_tiles --split=9 --recursive --jobs=8
```