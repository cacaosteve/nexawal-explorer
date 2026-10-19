use nexawal_explorer::{model::decode_transaction, rpc::Rpc};

#[tokio::test]
#[ignore = "requires explicit EXPLORER_TEST_RPC_URL; read-only public-node integration test"]
async fn public_node_smoke() {
    let url = std::env::var("EXPLORER_TEST_RPC_URL").expect("set EXPLORER_TEST_RPC_URL explicitly");
    let rpc = Rpc::new(&url).unwrap();
    let snapshot = rpc.snapshot().await.unwrap();
    let height = snapshot.tip.height.saturating_sub(10);
    let headers = rpc.headers(&snapshot, height, height + 1).await.unwrap();
    assert_eq!(headers.len(), 2);
    let b = rpc.block(&snapshot, &height.to_string()).await.unwrap();
    let by_hash = rpc.block(&snapshot, &b.block_header.hash).await.unwrap();
    assert_eq!(b.block_header.hash, by_hash.block_header.hash);
    let id = b.tx_hashes.first().unwrap_or(&b.miner_tx_hash);
    let entry = rpc
        .transaction(&snapshot, id)
        .await
        .unwrap()
        .expect("block's transaction must exist");
    let details = decode_transaction(&entry, id).unwrap();
    assert!(details.fee.is_some());
    assert!(b.contains(id));
    let observation = nexawal_explorer::api::transaction(&rpc, id).await.unwrap();
    assert_eq!(observation.api_version, 1);
    assert_eq!(observation.txid, *id);
    assert_eq!(observation.status, "confirmed");
    let advanced = nexawal_explorer::api::transaction_page(&rpc, id, Some(0))
        .await
        .unwrap();
    let page = advanced.details.unwrap();
    assert_eq!(page.page, 0);
    assert_eq!(page.page_size, 10);
    assert!(page.inputs.len() <= 10 && page.outputs.len() <= 10);
    assert!(page
        .inputs
        .iter()
        .all(|row| row.absolute_indices.len() <= 128));
    assert!(page
        .outputs
        .iter()
        .all(|row| page.coinbase || row.public_amount_piconero.is_none()));
    assert_eq!(observation.fee_piconero, details.fee.map(|f| f.to_string()));
    assert_eq!(observation.block_height, Some(height));
    assert_eq!(
        observation.block_hash.as_deref(),
        Some(b.block_header.hash.as_str())
    );
    let miner = rpc
        .transaction(&snapshot, &b.miner_tx_hash)
        .await
        .unwrap()
        .unwrap();
    let miner_details = decode_transaction(&miner, &b.miner_tx_hash).unwrap();
    assert!(miner_details.coinbase);
    assert_eq!(miner_details.fee, Some(0));
    let genesis = rpc.block(&snapshot, "0").await.unwrap();
    let first_miner = rpc
        .transaction(&snapshot, &genesis.miner_tx_hash)
        .await
        .unwrap()
        .unwrap();
    assert!(
        decode_transaction(&first_miner, &genesis.miner_tx_hash)
            .unwrap()
            .coinbase
    );
    assert!(rpc
        .transaction(&snapshot, &format!("{:064x}", 1))
        .await
        .unwrap()
        .is_none());
    let hashes = rpc.pool_hashes(&snapshot).await.unwrap();
    let entries = rpc
        .pool_entries(&snapshot, &hashes[..hashes.len().min(2)])
        .await
        .unwrap();
    for entry in &entries {
        decode_transaction(entry, &entry.tx_hash).unwrap();
    }
    println!(
        "Read-only smoke passed: network={}, height={}, pool sample={}",
        snapshot.info.nettype,
        snapshot.info.height,
        entries.len()
    );
}
