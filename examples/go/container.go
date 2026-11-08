package main

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"fmt"
	"io"

	"example.com/avrox-go/profile"
)

type avroFile struct {
	schema   []byte
	sync     [16]byte
	profiles []profile.UserProfile
}

func readAvroFile(input io.Reader) (avroFile, error) {
	r := bufio.NewReader(input)
	var result avroFile
	var magic [4]byte
	if _, err := io.ReadFull(r, magic[:]); err != nil {
		return result, err
	}
	if string(magic[:]) != "Obj\x01" {
		return result, fmt.Errorf("not an Avro object container file")
	}
	metadata, err := readMetadata(r)
	if err != nil {
		return result, fmt.Errorf("read Avro metadata: %w", err)
	}
	if codec := string(metadata["avro.codec"]); codec != "" && codec != "null" {
		return result, fmt.Errorf("unsupported Avro codec %q", codec)
	}
	schema, ok := metadata["avro.schema"]
	if !ok {
		return result, fmt.Errorf("Avro metadata has no schema")
	}
	result.schema = schema
	if _, err := io.ReadFull(r, result.sync[:]); err != nil {
		return result, err
	}

	for {
		count, err := binary.ReadVarint(r)
		if err == io.EOF {
			return result, nil
		}
		if err != nil {
			return avroFile{}, err
		}
		if count <= 0 {
			return avroFile{}, fmt.Errorf("invalid Avro object count %d", count)
		}
		size, err := binary.ReadVarint(r)
		if err != nil {
			return avroFile{}, err
		}
		block, err := readSizedBytes(r, size)
		if err != nil {
			return avroFile{}, err
		}
		blockReader := bytes.NewReader(block)
		for i := int64(0); i < count; i++ {
			var value profile.UserProfile
			if err := value.ReadAvro(blockReader); err != nil {
				return avroFile{}, fmt.Errorf("read profile %d: %w", len(result.profiles)+1, err)
			}
			result.profiles = append(result.profiles, value)
		}
		if blockReader.Len() != 0 {
			return avroFile{}, fmt.Errorf("Avro block contains %d trailing bytes", blockReader.Len())
		}
		var sync [16]byte
		if _, err := io.ReadFull(r, sync[:]); err != nil {
			return avroFile{}, err
		}
		if sync != result.sync {
			return avroFile{}, fmt.Errorf("Avro block sync marker mismatch")
		}
	}
}

func writeAvroFile(w io.Writer, schema []byte, sync [16]byte, profiles []profile.UserProfile) error {
	if _, err := w.Write([]byte("Obj\x01")); err != nil {
		return err
	}
	if err := writeLong(w, 2); err != nil {
		return err
	}
	if err := writeString(w, "avro.codec"); err != nil {
		return err
	}
	if err := writeBytes(w, []byte("null")); err != nil {
		return err
	}
	if err := writeString(w, "avro.schema"); err != nil {
		return err
	}
	if err := writeBytes(w, schema); err != nil {
		return err
	}
	if err := writeLong(w, 0); err != nil {
		return err
	}
	if _, err := w.Write(sync[:]); err != nil {
		return err
	}

	if len(profiles) == 0 {
		return nil
	}
	var block bytes.Buffer
	for i := range profiles {
		if err := profiles[i].WriteAvro(&block); err != nil {
			return fmt.Errorf("write profile %d: %w", i+1, err)
		}
	}
	if err := writeLong(w, int64(len(profiles))); err != nil {
		return err
	}
	if err := writeLong(w, int64(block.Len())); err != nil {
		return err
	}
	if _, err := w.Write(block.Bytes()); err != nil {
		return err
	}
	_, err := w.Write(sync[:])
	return err
}

func readMetadata(r *bufio.Reader) (map[string][]byte, error) {
	metadata := map[string][]byte{}
	for {
		count, err := binary.ReadVarint(r)
		if err != nil {
			return nil, err
		}
		if count == 0 {
			return metadata, nil
		}
		if count < 0 {
			count = -count
			if _, err := binary.ReadVarint(r); err != nil {
				return nil, err
			}
		}
		for i := int64(0); i < count; i++ {
			key, err := readString(r)
			if err != nil {
				return nil, err
			}
			value, err := readBytes(r)
			if err != nil {
				return nil, err
			}
			metadata[key] = value
		}
	}
}

func readString(r *bufio.Reader) (string, error) {
	value, err := readBytes(r)
	return string(value), err
}

func readBytes(r *bufio.Reader) ([]byte, error) {
	size, err := binary.ReadVarint(r)
	if err != nil {
		return nil, err
	}
	return readSizedBytes(r, size)
}

func readSizedBytes(r io.Reader, size int64) ([]byte, error) {
	if size < 0 {
		return nil, fmt.Errorf("invalid byte string size %d", size)
	}
	var value bytes.Buffer
	_, err := io.CopyN(&value, r, size)
	return value.Bytes(), err
}

func writeString(w io.Writer, value string) error {
	return writeBytes(w, []byte(value))
}

func writeBytes(w io.Writer, value []byte) error {
	if err := writeLong(w, int64(len(value))); err != nil {
		return err
	}
	_, err := w.Write(value)
	return err
}

func writeLong(w io.Writer, value int64) error {
	var data [binary.MaxVarintLen64]byte
	n := binary.PutVarint(data[:], value)
	_, err := w.Write(data[:n])
	return err
}
