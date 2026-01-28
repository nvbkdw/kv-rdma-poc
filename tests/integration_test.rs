//! Integration tests for KV Cache with RDMA

use kv_rdma_poc::client::{ClientConfig, KvCacheClient};
use kv_rdma_poc::server::{KvCacheServer, ServerConfig};
use kv_rdma_poc::transport::TransportConfig;
use std::time::Duration;

/// Find an available port for testing
fn find_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[tokio::test]
async fn test_server_client_integration() {
    // Initialize tracing for debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("kv_rdma_poc=debug")
        .try_init();

    // Find an available port
    let port = find_available_port();
    let server_addr = format!("[::1]:{}", port);
    let client_addr = format!("http://[::1]:{}", port);

    // Start server in background
    let server_config = ServerConfig {
        node_id: 0,
        listen_addr: server_addr.clone(),
        memory_pool_size: 16 * 1024 * 1024, // 16MB for testing
        transport: TransportConfig {
            node_id: 0,
            num_domains: 1,
            use_mock: true,
        },
    };

    let server = KvCacheServer::new(server_config.clone()).unwrap();
    let service = server.into_service();

    let server_handle = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service)
            .serve(server_addr.parse().unwrap())
            .await
            .unwrap();
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create and connect client
    let client_config = ClientConfig {
        client_id: 1,
        server_addr: client_addr,
        receive_buffer_size: 4 * 1024 * 1024, // 4MB
        transport: TransportConfig {
            node_id: 1,
            num_domains: 1,
            use_mock: true,
        },
    };

    let client = KvCacheClient::new(client_config).unwrap();
    client.connect().await.unwrap();

    // Test PUT
    client.put(b"key1", b"value1", 0).await.unwrap();
    client.put(b"key2", b"hello world", 0).await.unwrap();

    // Test GET
    let value1 = client.get(b"key1").await.unwrap();
    assert_eq!(value1, b"value1");

    let value2 = client.get(b"key2").await.unwrap();
    assert_eq!(value2, b"hello world");

    // Test GET non-existent key
    let result = client.get(b"nonexistent").await;
    assert!(result.is_err());

    // Test DELETE
    let existed = client.delete(b"key1").await.unwrap();
    assert!(existed);

    // Verify key is deleted
    let result = client.get(b"key1").await;
    assert!(result.is_err());

    // Test DELETE non-existent key
    let existed = client.delete(b"nonexistent").await.unwrap();
    assert!(!existed);

    // Test heartbeat
    let alive = client.heartbeat().await.unwrap();
    assert!(alive);

    // Clean up
    server_handle.abort();
}

#[tokio::test]
async fn test_large_values() {
    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .try_init();

    let port = find_available_port();
    let server_addr = format!("[::1]:{}", port);
    let client_addr = format!("http://[::1]:{}", port);

    // Start server
    let server_config = ServerConfig {
        node_id: 0,
        listen_addr: server_addr.clone(),
        memory_pool_size: 64 * 1024 * 1024,
        transport: TransportConfig::default(),
    };

    let server = KvCacheServer::new(server_config).unwrap();
    let service = server.into_service();

    let server_handle = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service)
            .serve(server_addr.parse().unwrap())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create client
    let client_config = ClientConfig {
        client_id: 1,
        server_addr: client_addr,
        receive_buffer_size: 16 * 1024 * 1024,
        transport: TransportConfig::default(),
    };

    let client = KvCacheClient::new(client_config).unwrap();
    client.connect().await.unwrap();

    // Test with various value sizes
    for size in [1024, 4096, 16384, 32768] {
        let key = format!("key_size_{}", size);
        let value: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

        client.put(key.as_bytes(), &value, 0).await.unwrap();

        let retrieved = client.get(key.as_bytes()).await.unwrap();
        assert_eq!(retrieved.len(), size);
        assert_eq!(retrieved, value);
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_multiple_clients() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .try_init();

    let port = find_available_port();
    let server_addr = format!("[::1]:{}", port);
    let client_addr = format!("http://[::1]:{}", port);

    // Start server
    let server_config = ServerConfig {
        node_id: 0,
        listen_addr: server_addr.clone(),
        memory_pool_size: 32 * 1024 * 1024,
        transport: TransportConfig::default(),
    };

    let server = KvCacheServer::new(server_config).unwrap();
    let service = server.into_service();

    let server_handle = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service)
            .serve(server_addr.parse().unwrap())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create multiple clients
    let mut clients = Vec::new();
    for i in 1..=3 {
        let client_config = ClientConfig {
            client_id: i,
            server_addr: client_addr.clone(),
            receive_buffer_size: 4 * 1024 * 1024,
            transport: TransportConfig {
                node_id: i,
                num_domains: 1,
                use_mock: true,
            },
        };

        let client = KvCacheClient::new(client_config).unwrap();
        client.connect().await.unwrap();
        clients.push(client);
    }

    // Client 1 writes, all clients read
    clients[0].put(b"shared_key", b"shared_value", 0).await.unwrap();

    for client in &clients {
        let value = client.get(b"shared_key").await.unwrap();
        assert_eq!(value, b"shared_value");
    }

    // Each client writes its own key
    for (i, client) in clients.iter().enumerate() {
        let key = format!("client_{}_key", i);
        let value = format!("client_{}_value", i);
        client.put(key.as_bytes(), value.as_bytes(), 0).await.unwrap();
    }

    // All clients can read all keys
    for (i, _) in clients.iter().enumerate() {
        for client in &clients {
            let key = format!("client_{}_key", i);
            let expected_value = format!("client_{}_value", i);
            let value = client.get(key.as_bytes()).await.unwrap();
            assert_eq!(value, expected_value.as_bytes());
        }
    }

    server_handle.abort();
}
