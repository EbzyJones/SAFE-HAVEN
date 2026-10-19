// ============================================================
//  Prediction Markets — Resolution Data Storage Tests (Issue #99)
// ============================================================
//
//  Scope:
//    - Resolution with data (Some(i128)) is accepted, stored, returned
//    - Resolution without data (None) is accepted, stored, returned
//    - resolution_data persists in get_prediction_market query
//    - OracleResolution record also stores the value
//
//  Out of scope (per issue):
//    - Data validation / custom formats / compression

#[cfg(test)]
mod prediction_market_tests {
    extern crate std;

    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    use crate::{
        contract::{SafeHaven, SafeHavenClient},
        prediction_market_types::MarketStatus,
    };

    // ----------------------------------------------------------------
    //  Test helpers
    // ----------------------------------------------------------------

    /// Advance the mock ledger clock by `seconds`.
    fn advance_time(env: &Env, seconds: u64) {
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + seconds,
            protocol_version: env.ledger().protocol_version(),
            sequence_number: env.ledger().sequence(),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 4096,
            max_entry_ttl: 33_000_000,
        });
    }

    /// Deploy contract, initialize both the vault and prediction market subsystem.
    /// Returns (env, client, token_addr, admin, alice, fee_recipient, oracle).
    fn setup() -> (
        Env,
        SafeHavenClient<'static>,
        Address,
        Address,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let vault_id = env.register(SafeHaven, ());
        let client = SafeHavenClient::new(&env, &vault_id);

        let admin: Address = Address::generate(&env);
        let alice: Address = Address::generate(&env);
        let fee_recipient: Address = Address::generate(&env);
        let oracle: Address = Address::generate(&env);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token_address = token_id.address();

        StellarAssetClient::new(&env, &token_address).mint(&alice, &100_000);

        // Initialize vault subsystem
        client.initialize(&admin, &fee_recipient, &None, &None);

        // Initialize prediction market subsystem
        client.init_prediction_markets(&admin, &fee_recipient, &None);

        (env, client, token_address, admin, alice, fee_recipient, oracle)
    }

    /// Create a market, return market_id.
    /// Advances time so close and resolution windows are valid.
    fn create_test_market(
        env: &Env,
        client: &SafeHavenClient,
        creator: &Address,
        oracle: &Address,
    ) -> u32 {
        let now = env.ledger().timestamp();
        let close_time = now + 7_200; // 2 hours from now (> MIN_MARKET_DURATION_SECS=3600)
        let resolution_deadline = close_time + 7_200; // 2 hours after close

        let mut outcome_names = Vec::new(env);
        outcome_names.push_back(String::from_str(env, "Yes"));
        outcome_names.push_back(String::from_str(env, "No"));

        client.create_prediction_market(
            creator,
            &String::from_str(env, "aggregate_deposits"),
            &String::from_str(env, "Will deposits exceed 1M?"),
            oracle,
            &close_time,
            &resolution_deadline,
            &outcome_names,
            &None,
        )
    }

    // ----------------------------------------------------------------
    //  Core Issue #99 Tests
    // ----------------------------------------------------------------

    /// Resolution with data: oracle passes Some(i128) — it must be stored and
    /// retrievable via get_prediction_market.
    #[test]
    fn test_resolve_market_stores_resolution_data_some() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);

        // Close market so oracle can resolve
        client.close_prediction_market(&oracle, &market_id);

        // Resolve with non-None data
        let data_value: i128 = 1_200_000_000; // e.g., 1.2M tokens
        client.resolve_prediction_market(
            &oracle,
            &market_id,
            &0,            // winning_outcome
            &Some(data_value),
        );

        // Query and verify
        let market = client
            .get_prediction_market(&market_id)
            .expect("market must exist after resolution");

        assert_eq!(
            market.resolution_data,
            Some(data_value),
            "resolution_data should be stored exactly as provided"
        );
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.winning_outcome, Some(0));
    }

    /// Resolution without data: oracle passes None — field must be None in storage.
    #[test]
    fn test_resolve_market_stores_resolution_data_none() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);
        client.close_prediction_market(&oracle, &market_id);

        // Resolve with no optional data
        client.resolve_prediction_market(
            &oracle,
            &market_id,
            &0,
            &None,
        );

        let market = client
            .get_prediction_market(&market_id)
            .expect("market must exist after resolution");

        assert_eq!(
            market.resolution_data,
            None,
            "resolution_data should be None when oracle provided None"
        );
        assert_eq!(market.status, MarketStatus::Resolved);
    }

    /// Data persistence: once stored, resolution_data must remain unchanged across
    /// multiple separate get_prediction_market calls (no mutation side effects).
    #[test]
    fn test_resolution_data_persists_across_queries() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);
        client.close_prediction_market(&oracle, &market_id);

        let stored_value: i128 = 42_000;
        client.resolve_prediction_market(&oracle, &market_id, &1, &Some(stored_value));

        // Query three times — value must be stable
        for _ in 0..3 {
            let market = client
                .get_prediction_market(&market_id)
                .expect("market must persist");
            assert_eq!(
                market.resolution_data,
                Some(stored_value),
                "resolution_data must be identical on every read"
            );
        }
    }

    /// Advancing the ledger after resolution should not affect stored data.
    #[test]
    fn test_resolution_data_persists_after_time_passes() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);
        client.close_prediction_market(&oracle, &market_id);

        let value: i128 = 999_888_777;
        client.resolve_prediction_market(&oracle, &market_id, &0, &Some(value));

        // Advance ledger significantly
        advance_time(&env, 1_000_000);

        let market = client
            .get_prediction_market(&market_id)
            .expect("market must still be accessible");
        assert_eq!(market.resolution_data, Some(value));
    }

    /// Boundary: resolution_data can hold i128::MAX without overflow.
    #[test]
    fn test_resolution_data_max_i128() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);
        client.close_prediction_market(&oracle, &market_id);

        let max_value: i128 = i128::MAX;
        client.resolve_prediction_market(&oracle, &market_id, &0, &Some(max_value));

        let market = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(market.resolution_data, Some(max_value));
    }

    /// Boundary: resolution_data can hold i128::MIN (negative values valid).
    #[test]
    fn test_resolution_data_min_i128() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);
        client.close_prediction_market(&oracle, &market_id);

        let min_value: i128 = i128::MIN;
        client.resolve_prediction_market(&oracle, &market_id, &0, &Some(min_value));

        let market = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(market.resolution_data, Some(min_value));
    }

    /// Boundary: resolution_data = Some(0) is valid and stored as zero.
    #[test]
    fn test_resolution_data_zero() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);
        client.close_prediction_market(&oracle, &market_id);

        client.resolve_prediction_market(&oracle, &market_id, &0, &Some(0));

        let market = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(market.resolution_data, Some(0));
    }

    /// Two independent markets store separate resolution_data values — no cross
    /// contamination.
    #[test]
    fn test_resolution_data_isolated_per_market() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_a = create_test_market(&env, &client, &alice, &oracle);

        // Create a second market with a different close/deadline window
        let now = env.ledger().timestamp();
        let mut outcome_names = Vec::new(&env);
        outcome_names.push_back(String::from_str(&env, "Yes"));
        outcome_names.push_back(String::from_str(&env, "No"));
        let market_b = client.create_prediction_market(
            &alice,
            &String::from_str(&env, "unique_depositors"),
            &String::from_str(&env, "Depositors > 500?"),
            &oracle,
            &(now + 8_000),
            &(now + 20_000),
            &outcome_names,
            &None,
        );

        client.close_prediction_market(&oracle, &market_a);
        client.close_prediction_market(&oracle, &market_b);

        let value_a: i128 = 111_111;
        let value_b: i128 = 222_222;

        client.resolve_prediction_market(&oracle, &market_a, &0, &Some(value_a));
        client.resolve_prediction_market(&oracle, &market_b, &1, &Some(value_b));

        let ma = client.get_prediction_market(&market_a).unwrap();
        let mb = client.get_prediction_market(&market_b).unwrap();

        assert_eq!(ma.resolution_data, Some(value_a));
        assert_eq!(mb.resolution_data, Some(value_b));
    }

    /// Before resolution, resolution_data in the market struct must be None.
    #[test]
    fn test_resolution_data_is_none_before_resolution() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);

        // Market still open — query should show None
        let market = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(
            market.resolution_data, None,
            "resolution_data must be None before the market is resolved"
        );
        assert_eq!(market.status, MarketStatus::Open);
    }

    /// Before resolution, resolution_data in the market struct must be None (closed state).
    #[test]
    fn test_resolution_data_is_none_when_closed_not_resolved() {
        let (env, client, _token, _admin, alice, _fee, oracle) = setup();

        let market_id = create_test_market(&env, &client, &alice, &oracle);
        client.close_prediction_market(&oracle, &market_id);

        let market = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(
            market.resolution_data, None,
            "resolution_data must be None before oracle submits resolution"
        );
        assert_eq!(market.status, MarketStatus::Closed);
    }

    /// Full lifecycle: create → close → resolve with data → query confirms data.
    #[test]
    fn test_full_lifecycle_resolution_data_stored() {
        let (env, client, token, admin, alice, fee_recipient, oracle) = setup();

        // Create market
        let now = env.ledger().timestamp();
        let close_time = now + 10_000;
        let resolution_deadline = close_time + 5_000;

        let mut outcome_names = Vec::new(&env);
        outcome_names.push_back(String::from_str(&env, "High (>1M)"));
        outcome_names.push_back(String::from_str(&env, "Low (<=1M)"));

        let market_id = client.create_prediction_market(
            &alice,
            &String::from_str(&env, "aggregate_deposits"),
            &String::from_str(&env, "Will deposits exceed 1M USDC?"),
            &oracle,
            &close_time,
            &resolution_deadline,
            &outcome_names,
            &Some(100), // 1% fee
        );

        // Verify market starts without resolution_data
        let before = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(before.resolution_data, None);
        assert_eq!(before.status, MarketStatus::Open);

        // Close market
        client.close_prediction_market(&oracle, &market_id);

        let closed = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(closed.resolution_data, None);
        assert_eq!(closed.status, MarketStatus::Closed);

        // Oracle resolves with data
        let final_aggregate: i128 = 1_500_000_000_000; // 1.5M with 6 decimal places
        client.resolve_prediction_market(
            &oracle,
            &market_id,
            &0, // "High" wins
            &Some(final_aggregate),
        );

        // Verify full state after resolution
        let resolved = client.get_prediction_market(&market_id).unwrap();
        assert_eq!(resolved.status, MarketStatus::Resolved);
        assert_eq!(resolved.winning_outcome, Some(0));
        assert_eq!(
            resolved.resolution_data,
            Some(final_aggregate),
            "resolution_data must equal the value submitted by the oracle"
        );
    }
}
