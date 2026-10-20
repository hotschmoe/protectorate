package main

import (
	"fmt"
	"sync"
)

func main() {
	var shared int
	var done sync.WaitGroup
	start := make(chan struct{})
	for range 2 {
		done.Go(func() {
			<-start
			for range 10000 {
				shared++
			}
		})
	}
	close(start)
	done.Wait()
	fmt.Println("race peer completed")
}
