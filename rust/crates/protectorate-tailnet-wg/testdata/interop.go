// This helper checks packet interoperability against the cached wireguard-go
// module. Run through the ignored Rust integration test in tests/interop.rs.
package main

import (
    "bufio"
    "bytes"
    "encoding/binary"
    "encoding/hex"
    "fmt"
    "os"

    "golang.org/x/crypto/blake2s"
    "golang.org/x/crypto/curve25519"
    "golang.zx2c4.com/wireguard/conn"
    "golang.zx2c4.com/wireguard/device"
    "golang.zx2c4.com/wireguard/tun/tuntest"
)

func fail(err error) {
    if err != nil { panic(err) }
}

func mac1(packet []byte, receiver []byte) {
    h, err := blake2s.New256(nil); fail(err)
    h.Write([]byte("mac1----")); h.Write(receiver)
    key := h.Sum(nil)
    m, err := blake2s.New128(key); fail(err)
    m.Write(packet[:len(packet)-32])
    copy(packet[len(packet)-32:len(packet)-16], m.Sum(nil))
}

func main() {
    if len(os.Args) != 2 { panic("expected initiate or respond") }
    local := bytes.Repeat([]byte{2}, 32)
    remote := bytes.Repeat([]byte{1}, 32)
    if os.Args[1] == "initiate" { local, remote = remote, local }
    localPK, err := curve25519.X25519(local, curve25519.Basepoint); fail(err)
    remotePK, err := curve25519.X25519(remote, curve25519.Basepoint); fail(err)
    var sk device.NoisePrivateKey; copy(sk[:], local)
    var pk device.NoisePublicKey; copy(pk[:], remotePK)
    tun := tuntest.NewChannelTUN()
    dev := device.NewDevice(tun.TUN(), conn.NewDefaultBind(), device.NewLogger(device.LogLevelError, ""))
    defer dev.Close()
    fail(dev.SetPrivateKey(sk))
    peer, err := dev.NewPeer(pk); fail(err)
    peer.Start()

    if os.Args[1] == "initiate" {
        msg, err := dev.CreateMessageInitiation(peer); fail(err)
        buf := new(bytes.Buffer)
        fail(binary.Write(buf, binary.LittleEndian, msg))
        packet := buf.Bytes()
        mac1(packet, remotePK)
        fmt.Println(hex.EncodeToString(packet))
        return
    }
    input, err := bufio.NewReader(os.Stdin).ReadString('\n'); fail(err)
    packet, err := hex.DecodeString(input[:len(input)-1]); fail(err)
    var init device.MessageInitiation
    fail(binary.Read(bytes.NewReader(packet), binary.LittleEndian, &init))
    if dev.ConsumeMessageInitiation(&init) == nil { panic("wireguard-go rejected Rust initiation") }
    reply, err := dev.CreateMessageResponse(peer); fail(err)
    buf := new(bytes.Buffer)
    fail(binary.Write(buf, binary.LittleEndian, reply))
    answer := buf.Bytes()
    mac1(answer, remotePK)
    fmt.Println(hex.EncodeToString(answer))
    _ = localPK
}
