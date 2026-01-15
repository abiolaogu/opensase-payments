# opensase-payments

Self-hosted Payment Processing system built with Rust.

## Features

- Payment initiation (Paystack, Flutterwave ready)
- Transaction tracking
- Wallet management
- Refund processing
- Webhook handling

## Quick Start

```bash
docker-compose up -d
curl http://localhost:8084/health
```

## API Endpoints

- `POST /api/v1/payments/initiate` - Start payment
- `POST /api/v1/payments/verify` - Verify payment
- `GET /api/v1/transactions` - List transactions
- `POST /api/v1/refunds` - Create refund
- `POST /api/v1/wallets` - Create wallet
- `POST /api/v1/wallets/:id/topup` - Top up wallet
- `POST /api/v1/transfers` - Wallet transfer

## License

MIT OR Apache-2.0
