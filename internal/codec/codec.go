// Package codec holds the wire codec shared by every Connect service.
package codec

import (
	"errors"
	"fmt"

	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
)

// StrictJSON replaces connect's JSON codec so unknown request fields are an
// error instead of being silently dropped. Register it with connect.WithCodec.
type StrictJSON struct{}

func (StrictJSON) Name() string { return "json" }

func (StrictJSON) Marshal(m any) ([]byte, error) {
	pm, ok := m.(proto.Message)
	if !ok {
		return nil, fmt.Errorf("%T is not a proto message", m)
	}
	return protojson.Marshal(pm)
}

func (StrictJSON) Unmarshal(data []byte, m any) error {
	pm, ok := m.(proto.Message)
	if !ok {
		return fmt.Errorf("%T is not a proto message", m)
	}
	if len(data) == 0 {
		return errors.New("zero-length payload is not a valid JSON object")
	}
	return protojson.UnmarshalOptions{DiscardUnknown: false}.Unmarshal(data, pm)
}

func (StrictJSON) IsBinary() bool { return false }
