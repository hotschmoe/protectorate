package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"time"

	"connectrpc.com/connect"

	pb "spike/connect/gen/protectorate/v1"
	"spike/connect/gen/protectorate/v1/protectoratev1connect"
)

func main() {
	ctx := context.Background()
	base := "http://127.0.0.1:7471"
	sl := protectoratev1connect.NewSleeveServiceClient(http.DefaultClient, base, connect.WithProtoJSON())
	ev := protectoratev1connect.NewEventServiceClient(http.DefaultClient, base, connect.WithProtoJSON())

	stream, err := ev.Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
	if err != nil {
		log.Fatal(err)
	}
	go func() {
		for stream.Receive() {
			fmt.Println("event:", stream.Msg())
		}
		fmt.Println("stream ended:", stream.Err())
	}()

	r, err := sl.Spawn(ctx, connect.NewRequest(&pb.SpawnRequest{Name: "alice", Harness: "claude"}))
	fmt.Println("spawn:", r.Msg, err)
	_, err = sl.Spawn(ctx, connect.NewRequest(&pb.SpawnRequest{Name: "alice"}))
	fmt.Println("dup spawn code:", connect.CodeOf(err), err)
	l, _ := sl.List(ctx, connect.NewRequest(&pb.ListRequest{}))
	fmt.Println("list:", l.Msg)
	time.Sleep(2500 * time.Millisecond)
}
