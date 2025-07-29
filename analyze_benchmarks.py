#!/usr/bin/env python3
"""
Script to systematically analyze Criterion benchmark results from target/criterion/
Extracts performance data and compares versioned ART vs im::HashMap vs im::OrdMap
"""

import json
import os
import sys
from pathlib import Path
from typing import Dict, List, Tuple, Optional

def parse_estimates(file_path: Path) -> Optional[Dict]:
    """Parse estimates.json file and return benchmark data"""
    try:
        with open(file_path, 'r') as f:
            data = json.load(f)
        return {
            'mean': data['mean']['point_estimate'],
            'median': data['median']['point_estimate'], 
            'std_dev': data['std_dev']['point_estimate'],
            'confidence_interval': {
                'lower': data['mean']['confidence_interval']['lower_bound'],
                'upper': data['mean']['confidence_interval']['upper_bound']
            }
        }
    except (FileNotFoundError, json.JSONDecodeError, KeyError) as e:
        print(f"Error parsing {file_path}: {e}")
        return None

def collect_benchmark_data(criterion_dir: Path) -> Dict:
    """Collect all benchmark data organized by benchmark type and implementation"""
    benchmarks = {}
    
    # Walk through criterion directory structure
    for bench_dir in criterion_dir.iterdir():
        if not bench_dir.is_dir():
            continue
            
        bench_name = bench_dir.name
        benchmarks[bench_name] = {}
        
        # Look for implementation subdirs (versioned_art, im_hashmap, im_ordmap)
        for impl_dir in bench_dir.iterdir():
            if not impl_dir.is_dir():
                continue
                
            impl_name = impl_dir.name
            benchmarks[bench_name][impl_name] = {}
            
            # Look for parameter subdirs (sizes, counts, etc.)
            for param_dir in impl_dir.iterdir():
                if not param_dir.is_dir():
                    continue
                    
                param_value = param_dir.name
                estimates_file = param_dir / "new" / "estimates.json"
                
                if estimates_file.exists():
                    data = parse_estimates(estimates_file)
                    if data:
                        benchmarks[bench_name][impl_name][param_value] = data
    
    return benchmarks

def format_time(nanoseconds: float) -> str:
    """Format nanoseconds into readable time units"""
    if nanoseconds < 1000:
        return f"{nanoseconds:.1f} ns"
    elif nanoseconds < 1_000_000:
        return f"{nanoseconds/1000:.1f} µs"
    elif nanoseconds < 1_000_000_000:
        return f"{nanoseconds/1_000_000:.1f} ms"
    else:
        return f"{nanoseconds/1_000_000_000:.1f} s"

def analyze_benchmark(bench_name: str, data: Dict) -> None:
    """Analyze a single benchmark and print results"""
    print(f"\n{'='*60}")
    print(f"BENCHMARK: {bench_name}")
    print(f"{'='*60}")
    
    if not data:
        print("No data available")
        return
    
    # Get all parameter values across implementations
    all_params = set()
    for impl_data in data.values():
        all_params.update(impl_data.keys())
    
    # Sort parameters (try numeric first, then string)
    try:
        sorted_params = sorted(all_params, key=int)
    except ValueError:
        sorted_params = sorted(all_params)
    
    print(f"{'Parameter':<12} {'VersionedART':<15} {'ImHashMap':<15} {'ImOrdMap':<15} {'Best':<15}")
    print("-" * 80)
    
    for param in sorted_params:
        row = [param]
        times = {}
        
        # Collect times for each implementation
        for impl in ['versioned_art', 'im_hashmap', 'im_ordmap']:
            if impl in data and param in data[impl]:
                time_ns = data[impl][param]['mean']
                times[impl] = time_ns
                row.append(format_time(time_ns))
            else:
                row.append("N/A")
        
        # Determine winner
        if times:
            winner = min(times, key=times.get)
            winner_display = {'versioned_art': 'VersionedART', 'im_hashmap': 'ImHashMap', 'im_ordmap': 'ImOrdMap'}[winner]
            row.append(winner_display)
        else:
            row.append("N/A")
        
        print(f"{row[0]:<12} {row[1]:<15} {row[2]:<15} {row[3]:<15} {row[4]:<15}")
        
    # Performance ratios analysis
    print(f"\nPerformance Ratios (relative to VersionedART):")
    print(f"{'Parameter':<12} {'ImHashMap/ART':<15} {'ImOrdMap/ART':<15}")
    print("-" * 45)
    
    for param in sorted_params:
        if 'versioned_art' not in data or param not in data['versioned_art']:
            continue
            
        art_time = data['versioned_art'][param]['mean']
        ratios = [param]
        
        for impl in ['im_hashmap', 'im_ordmap']:
            if impl in data and param in data[impl]:
                impl_time = data[impl][param]['mean']
                ratio = impl_time / art_time
                ratios.append(f"{ratio:.2f}x")
            else:
                ratios.append("N/A")
        
        print(f"{ratios[0]:<12} {ratios[1]:<15} {ratios[2]:<15}")

def main():
    criterion_dir = Path("/Users/ryan/rart-rs/target/criterion")
    
    if not criterion_dir.exists():
        print(f"Criterion directory not found: {criterion_dir}")
        sys.exit(1)
    
    print("Collecting benchmark data...")
    benchmarks = collect_benchmark_data(criterion_dir)
    
    if not benchmarks:
        print("No benchmark data found")
        sys.exit(1)
    
    print(f"Found {len(benchmarks)} benchmark categories:")
    for bench_name in sorted(benchmarks.keys()):
        print(f"  - {bench_name}")
    
    # Analyze each benchmark
    for bench_name in sorted(benchmarks.keys()):
        analyze_benchmark(bench_name, benchmarks[bench_name])
    
    # Summary analysis
    print(f"\n{'='*60}")
    print("SUMMARY")
    print(f"{'='*60}")
    
    art_wins = 0
    hashmap_wins = 0 
    ordmap_wins = 0
    total_comparisons = 0
    
    for bench_name, bench_data in benchmarks.items():
        if not bench_data:
            continue
            
        # Count wins per benchmark
        for param in bench_data.get('versioned_art', {}):
            times = {}
            for impl in ['versioned_art', 'im_hashmap', 'im_ordmap']:
                if impl in bench_data and param in bench_data[impl]:
                    times[impl] = bench_data[impl][param]['mean']
            
            if len(times) >= 2:  # Need at least 2 implementations to compare
                winner = min(times, key=times.get)
                total_comparisons += 1
                if winner == 'versioned_art':
                    art_wins += 1
                elif winner == 'im_hashmap':
                    hashmap_wins += 1
                elif winner == 'im_ordmap':
                    ordmap_wins += 1
    
    if total_comparisons > 0:
        print(f"Overall Performance Summary ({total_comparisons} comparisons):")
        print(f"  VersionedART wins: {art_wins} ({art_wins/total_comparisons*100:.1f}%)")
        print(f"  ImHashMap wins:    {hashmap_wins} ({hashmap_wins/total_comparisons*100:.1f}%)")
        print(f"  ImOrdMap wins:     {ordmap_wins} ({ordmap_wins/total_comparisons*100:.1f}%)")

if __name__ == "__main__":
    main()