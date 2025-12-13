# Shopping Cart REST Server

A complete example demonstrating:
- HTTP REST API with multiple endpoints
- SQLite persistence with prepared statements
- Database transactions for checkout
- URL query parameter parsing
- Concurrent request handling with strands

## Build

```bash
seqc --ffi-manifest examples/shopping-cart/sqlite.toml \
     examples/shopping-cart/shopping-cart.seq -o shopping-cart
```

## Run

```bash
./shopping-cart
```

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | API info |
| GET | `/products` | List all products |
| GET | `/cart` | View cart contents |
| POST | `/cart/add?product=ID&qty=N` | Add item to cart |
| POST | `/cart/remove?id=N` | Remove item from cart |
| POST | `/cart/checkout` | Process order with transaction |

## Test with curl

```bash
# List products
curl http://localhost:8080/products

# Add items to cart
curl -X POST "http://localhost:8080/cart/add?product=1&qty=2"
curl -X POST "http://localhost:8080/cart/add?product=4&qty=1"

# View cart
curl http://localhost:8080/cart

# Checkout (uses transaction)
curl -X POST http://localhost:8080/cart/checkout

# Verify stock was updated
curl http://localhost:8080/products
```

## Database

The server creates `shop.db` with three tables:
- `products` - Product catalog with stock tracking
- `cart_items` - Shopping cart
- `orders` - Completed orders

Check the database directly:
```bash
sqlite3 shop.db "SELECT * FROM products"
sqlite3 shop.db "SELECT * FROM orders"
```

## Features Demonstrated

### SQLite FFI
- `db-open`, `db-close` - Connection management
- `db-exec` - Simple SQL execution
- `db-prepare`, `db-step`, `db-finalize` - Prepared statements
- `db-column-int`, `db-column-text` - Result extraction

### Transactions
The checkout process uses `BEGIN TRANSACTION` / `COMMIT` / `ROLLBACK`:
1. Calculate cart total
2. Begin transaction
3. Update product stock
4. Create order record
5. Clear cart
6. Commit (or rollback on error)

### HTTP Server
- Route matching by method and path
- Query parameter parsing
- Concurrent request handling with strands
