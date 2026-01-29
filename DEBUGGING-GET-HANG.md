# Debugging GET Operation Hang

The GET operation is hanging during RDMA transfer. This guide will help you diagnose the issue.

## What We Know

✅ Server starts successfully with EFA
✅ Client connects and registers
✅ PUT operations work (data is stored)
❌ GET operations hang (RDMA read-back fails)

## Debug Logging Added

I've added extensive debug logging to both client and server. Rebuild and run with:

```bash
cargo build --release --features rdma
```

## Running with Debug Logs

### Option 1: Use the debug script

```bash
# Server (Machine 1)
./test-rdma-debug.sh server

# Client (Machine 2 - run this AFTER server is up)
./test-rdma-debug.sh client <server-ip>
```

### Option 2: Manual with RUST_LOG

```bash
# Server
RUST_LOG=debug ./run-with-rdma.sh ./target/release/kv-server \
  --listen-addr "0.0.0.0:50051" \
  --log-level debug

# Client
RUST_LOG=debug ./run-with-rdma.sh ./target/release/kv-client \
  --server-addr "http://<server-ip>:50051" \
  --log-level debug \
  get mykey
```

## What to Look For

### Client Side Logs (in order)
1. `DEBUG kv_rdma_poc::client: GET: Starting request`
2. `DEBUG kv_rdma_poc::client: GET: Allocating receive buffer`
3. `DEBUG kv_rdma_poc::client: GET: Created response location, ptr=...`
4. `DEBUG kv_rdma_poc::client: GET: Sending gRPC request`
5. **← HANGS HERE IF RDMA DOESN'T COMPLETE** ←
6. `DEBUG kv_rdma_poc::client: GET: Received gRPC response`
7. `INFO kv_rdma_poc::client: GET: Successfully retrieved value`

### Server Side Logs (in order)
1. `DEBUG kv_rdma_poc::server: GET request: key=...`
2. `DEBUG kv_rdma_poc::server: GET: Looking up key`
3. `DEBUG kv_rdma_poc::server: GET: Found value, length=...`
4. `DEBUG kv_rdma_poc::server: GET: Creating transfer request`
5. `DEBUG kv_rdma_poc::server: GET: Submitting RDMA write`
6. **← HANGS HERE IF fabric-lib DOESN'T COMPLETE** ←
7. `DEBUG kv_rdma_poc::server: GET: Transfer completed`
8. `INFO kv_rdma_poc::server: GET: Successfully transferred ... bytes`

## Common Root Causes

### 1. Peer Address Resolution Failure

**Symptom:** Server logs show "Submitting RDMA write" but never completes.

**Cause:** fabric-lib can't establish RDMA connection to client's domain address.

**Check:**
```bash
# On both machines - should show EFA address
fi_info -p efa | grep "src_addr\|dest_addr"
```

**Verify connectivity:**
```bash
# From client, can you ping server's private IP?
ping <server-private-ip>

# Are both instances in same subnet with EFA enabled?
```

### 2. Security Group Misconfiguration

**Symptom:** gRPC works but RDMA hangs.

**Cause:** Security group blocks EFA traffic.

**Fix:** Update security group to allow:
- **All traffic** from same security group (required for EFA)
- Specifically: All protocols, all ports from same SG
- TCP 50051 for gRPC (already working since PUT works)

**Verify:**
```bash
# Check security group allows inbound from itself
aws ec2 describe-security-groups --group-ids <sg-id> | grep IpProtocol
```

Should show rule like:
```json
{
  "IpProtocol": "-1",
  "UserIdGroupPairs": [{"GroupId": "<same-sg-id>"}]
}
```

### 3. EFA Device Not Properly Configured

**Check EFA status:**
```bash
# Should show device
ibv_devices

# Should show active state
ibv_devinfo | grep -A5 state

# Should show efa provider
fi_info -p efa
```

### 4. Memory Registration Issue

**Symptom:** Transfer submitted but callback never fires.

**Possible cause:** Client's memory descriptor not valid for remote write.

**Test:** Try inline transfer (PUT operation should work, GET might not).

### 5. fabric-lib Worker Thread Issue

**Check:** Look for panic in worker threads:
```
thread 'tx_engine_worker' panicked at...
```

If worker thread panics, transfers won't complete.

## Immediate Diagnostic Steps

Run these commands on BOTH machines:

```bash
# 1. Check EFA device
echo "=== EFA Device ==="
ibv_devices
echo ""

# 2. Check EFA provider
echo "=== EFA Provider ==="
fi_info -p efa | head -20
echo ""

# 3. Check security group
echo "=== Network Info ==="
TOKEN=$(curl -X PUT "http://169.254.169.254/latest/api/token" -H "X-aws-ec2-metadata-token-ttl-seconds: 21600" 2>/dev/null)
INSTANCE_ID=$(curl -H "X-aws-ec2-metadata-token: $TOKEN" http://169.254.169.254/latest/meta-data/instance-id 2>/dev/null)
SG_ID=$(aws ec2 describe-instances --instance-ids $INSTANCE_ID --query 'Reservations[0].Instances[0].SecurityGroups[0].GroupId' --output text 2>/dev/null)
echo "Instance: $INSTANCE_ID"
echo "Security Group: $SG_ID"
echo ""

# 4. Check if instances can reach each other
echo "=== Connectivity ==="
PRIVATE_IP=$(curl -H "X-aws-ec2-metadata-token: $TOKEN" http://169.254.169.254/latest/meta-data/local-ipv4 2>/dev/null)
echo "My private IP: $PRIVATE_IP"
echo "Can I reach other instance? (test from other side)"
```

## Next Steps

1. **Run debug script** on both machines
2. **Compare logs** - find where it stops
3. **Check security groups** - most common issue
4. **Verify EFA setup** - ensure both instances have working EFA

## If Still Stuck

Check if server receives the gRPC GET request at all:
- If server never logs "GET request", it's a network/gRPC issue
- If server logs "Submitting RDMA write" but hangs, it's an RDMA peer connection issue
- If both log their parts but never complete, it's likely security group blocking EFA

## Quick Fix to Test Theory

To verify it's specifically an RDMA data plane issue (not control plane):

1. Change the value size threshold in code to force inline transfer
2. If GET works with inline but not RDMA, confirms RDMA path is the problem
3. Then focus on EFA connectivity and security groups

## Files Created
- `test-rdma-debug.sh` - Automated debug test script
- This diagnostic guide
