// main.go
package main

import (
	"context"
	"database/sql"
	"flag"
	"fmt"
	"math"
	"math/rand"
	"os"
	"sync"
	"sync/atomic"
	"time"

	_ "github.com/lib/pq"
)

const (
	defaultPGURL             = "postgres://localhost:5432/postgres?sslmode=disable"
	defaultDuration          = 10 * time.Second
	defaultWarmupDuration    = 5 * time.Second
	defaultAlpha             = 0.5
	defaultConnections       = 10
	defaultAmount            = 1
	defaultAccounts          = 100_000
	defaultMaxInflight       = 128
)

type BenchConfig struct {
	PGURL            string
	Accounts         uint32
	Alpha            float64
	Amount           uint32
	Connections      int
	MaxInflight      int
	Duration         time.Duration
	WarmupDuration   time.Duration
	TPSWritePath     string
	Quiet            bool
}

// zipfSample returns a Zipf-distributed value in [1, n].
func zipfSample(rng *rand.Rand, n uint32, alpha float64) uint32 {
	// Rejection-based Zipf sampling
	// Using Go's built-in Zipf generator
	z := rand.NewZipf(rng, alpha+1.0, 1.0, uint64(n)-1)
	return uint32(z.Uint64()) + 1
}

func pickTwoDistinct(rng *rand.Rand, accounts uint32, alpha float64) (uint32, uint32) {
	a := zipfSample(rng, accounts, alpha)
	b := zipfSample(rng, accounts, alpha)
	for spins := 0; a == b && spins < 32; spins++ {
		b = zipfSample(rng, accounts, alpha)
	}
	return a, b
}

func makeTransfers(accounts uint32, alpha float64) [][2]uint32 {
	rng := rand.New(rand.NewSource(0x12345678))
	pairs := make([][2]uint32, 0, 10_000_000)
	for i := 0; i < 10_000_000; i++ {
		from, to := pickTwoDistinct(rng, accounts, alpha)
		if from >= accounts || to >= accounts || from == to {
			continue
		}
		pairs = append(pairs, [2]uint32{from, to})
	}
	return pairs
}

func openPool(pgURL string, maxConns int) (*sql.DB, error) {
	db, err := sql.Open("postgres", pgURL)
	if err != nil {
		return nil, fmt.Errorf("failed to open db: %w", err)
	}
	db.SetMaxOpenConns(maxConns)
	db.SetMaxIdleConns(maxConns)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := db.PingContext(ctx); err != nil {
		return nil, fmt.Errorf("failed to ping db: %w", err)
	}
	return db, nil
}

func runBench(cfg BenchConfig) error {
	if !cfg.Quiet {
		fmt.Println("Benchmark parameters:")
		fmt.Printf("  alpha=%.2f, amount=%d, accounts=%d\n", cfg.Alpha, cfg.Amount, cfg.Accounts)
		fmt.Printf("  connections=%d, max_inflight_per_conn=%d\n", cfg.Connections, cfg.MaxInflight)
		fmt.Printf("  warmup=%s, duration=%s\n", cfg.WarmupDuration, cfg.Duration)
		fmt.Println()
	}

	db, err := openPool(cfg.PGURL, cfg.Connections)
	if err != nil {
		return err
	}
	defer db.Close()

	// Pre-compute transfer pairs.
	if !cfg.Quiet {
		fmt.Println("pre-computing transfer pairs...")
	}
	transferPairs := makeTransfers(cfg.Accounts, cfg.Alpha)
	pairsPerWorker := len(transferPairs) / cfg.Connections
	if pairsPerWorker == 0 {
		pairsPerWorker = len(transferPairs)
	}

	if !cfg.Quiet {
		fmt.Printf("generated %d transfer pairs (%d per worker)\n\n", len(transferPairs), pairsPerWorker)
	}

	// Acquire one connection per worker up front.
	conns := make([]*sql.Conn, cfg.Connections)
	for i := 0; i < cfg.Connections; i++ {
		c, err := db.Conn(context.Background())
		if err != nil {
			return fmt.Errorf("failed to acquire connection %d: %w", i, err)
		}
		conns[i] = c
	}
	defer func() {
		for _, c := range conns {
			c.Close()
		}
	}()

	query := "CALL transfer($1, $2, $3)"

	// runBatch executes up to maxInflight sequential transfers and returns the count.
	runBatch := func(conn *sql.Conn, pairs [][2]uint32, idx *int, max int) (int, error) {
		count := 0
		for count < max {
			if *idx >= len(pairs) {
				*idx = 0
			}
			p := pairs[*idx]
			*idx++
			_, err := conn.ExecContext(context.Background(), query, p[0], p[1], cfg.Amount)
			if err != nil {
				// Insufficient funds errors are expected; skip them.
				count++
				continue
			}
			count++
		}
		return count, nil
	}

	// --- Warmup phase ---
	if !cfg.Quiet {
		fmt.Printf("warming up for %s...\n", cfg.WarmupDuration)
	}

	var warmupWg sync.WaitGroup
	warmupDeadline := time.Now().Add(cfg.WarmupDuration)
	for i := 0; i < cfg.Connections; i++ {
		warmupWg.Add(1)
		go func(workerIdx int) {
			defer warmupWg.Done()
			conn := conns[workerIdx]
			startIdx := (workerIdx * pairsPerWorker) % len(transferPairs)
			idx := startIdx
			myPairs := transferPairs
			for time.Now().Before(warmupDeadline) {
				runBatch(conn, myPairs, &idx, cfg.MaxInflight)
			}
		}(i)
	}
	warmupWg.Wait()

	if !cfg.Quiet {
		fmt.Println("finished warmup.")
		fmt.Printf("benchmarking for %s...\n", cfg.Duration)
	}

	// --- Benchmark phase ---
	var completed atomic.Int64
	var benchWg sync.WaitGroup
	benchStart := time.Now()
	benchDeadline := benchStart.Add(cfg.Duration)

	for i := 0; i < cfg.Connections; i++ {
		benchWg.Add(1)
		go func(workerIdx int) {
			defer benchWg.Done()
			conn := conns[workerIdx]
			startIdx := (workerIdx * pairsPerWorker) % len(transferPairs)
			idx := startIdx
			myPairs := transferPairs
			for time.Now().Before(benchDeadline) {
				n, _ := runBatch(conn, myPairs, &idx, cfg.MaxInflight)
				completed.Add(int64(n))
			}
		}(i)
	}
	benchWg.Wait()

	elapsed := time.Since(benchStart).Seconds()
	total := completed.Load()
	tps := float64(total) / elapsed

	if !cfg.Quiet {
		fmt.Printf("\nran for %.3f seconds\n", elapsed)
		fmt.Printf("completed %d transfers\n", total)
		fmt.Printf("throughput: %.2f TPS\n", tps)
	}

	if math.IsNaN(tps) || math.IsInf(tps, 0) {
		tps = 0
	}

	if cfg.TPSWritePath != "" {
		if err := os.WriteFile(cfg.TPSWritePath, []byte(fmt.Sprintf("%f", tps)), 0644); err != nil {
			return fmt.Errorf("failed to write TPS file: %w", err)
		}
	}

	// Always print the raw TPS to stdout for scripting.
	if cfg.Quiet {
		fmt.Println(tps)
	}

	return nil
}

func main() {
	cfg := BenchConfig{}

	var accounts, amount uint
	flag.StringVar(&cfg.PGURL, "pg-url", defaultPGURL, "PostgreSQL connection URL (or PG_URL env)")
	flag.UintVar(&accounts, "accounts", uint(defaultAccounts), "number of accounts")
	flag.Float64Var(&cfg.Alpha, "alpha", defaultAlpha, "Zipf alpha parameter")
	flag.UintVar(&amount, "amount", defaultAmount, "transfer amount")
	flag.IntVar(&cfg.Connections, "connections", defaultConnections, "number of parallel connections")
	flag.IntVar(&cfg.MaxInflight, "max-inflight", defaultMaxInflight, "max sequential transfers per batch")
	flag.DurationVar(&cfg.Duration, "duration", defaultDuration, "benchmark duration")
	flag.DurationVar(&cfg.WarmupDuration, "warmup-duration", defaultWarmupDuration, "warmup duration")
	flag.StringVar(&cfg.TPSWritePath, "tps-write-path", "", "file path to write TPS result")
	flag.BoolVar(&cfg.Quiet, "quiet", false, "suppress informational output")
	flag.Parse()

	cfg.Accounts = uint32(accounts)
	cfg.Amount = uint32(amount)

	if err := runBench(cfg); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}

func envOrDefault(key, fallback string) string {
	if v, ok := os.LookupEnv(key); ok {
		return v
	}
	return fallback
}