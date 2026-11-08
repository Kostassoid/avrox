package main

import (
	"bytes"
	"testing"

	"example.com/avrox-go/profile"
)

func TestEmptyContainerRoundTrip(t *testing.T) {
	schema := []byte(`{"type":"record","name":"UserProfile","fields":[]}`)
	var sync [16]byte
	var encoded bytes.Buffer
	if err := writeAvroFile(&encoded, schema, sync, []profile.UserProfile{}); err != nil {
		t.Fatal(err)
	}
	decoded, err := readAvroFile(&encoded)
	if err != nil {
		t.Fatal(err)
	}
	if len(decoded.profiles) != 0 {
		t.Fatalf("read %d profiles from an empty container", len(decoded.profiles))
	}
	if !bytes.Equal(decoded.schema, schema) {
		t.Fatal("schema changed during the round trip")
	}
}
