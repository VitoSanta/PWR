# cart

Cart pricing for the shop, PHP 8.2+, no runtime dependencies (PHPUnit for
the tests). Namespace `Shop`, PSR-4 from `src/`. Money is always integer
cents; wherever a percentage makes a fraction of a cent, it is rounded half
up (`PHP_ROUND_HALF_UP`).

```php
$catalog = new Shop\Catalog([
    'CHIPS' => ['price' => 250, 'category' => 'snacks', 'weight' => 150],
    'KETTLE' => ['price' => 3990, 'category' => 'kitchen', 'weight' => 1200],
]);
$cart = new Shop\Cart($catalog);
$cart->add('CHIPS', 3);
$cart->applyCoupon('SAVE10');
$cart->lines();   // [['sku' => 'CHIPS', 'qty' => 3, 'unit' => 250, 'total' => 750]]
$cart->totals();  // ['subtotal' => 750, 'discount' => ..., 'shipping' => ..., 'tax' => ..., 'total' => ...]
```

## Cart

- `add(string $sku, int $qty = 1)`: adds to the line of that SKU (one line
  per SKU, in the order first added). An unknown SKU or `$qty < 1` throws
  `InvalidArgumentException`.
- `remove(string $sku, int $qty = 1)`: takes units off; the line goes when it
  reaches 0 (removing more than there is removes the line). A SKU not in the
  cart or `$qty < 1` throws `InvalidArgumentException`.
- `lines()`: `sku`, `qty`, `unit` (price), `total` (`qty * unit`) per line.
- `applyCoupon(string $code)`: one coupon at a time, a new one replaces the
  old; codes are case-insensitive. An unknown code throws
  `InvalidArgumentException` and leaves the current coupon in place.
  `removeCoupon()` removes it.

## Totals

In this order:

1. `subtotal`: the sum of the lines' totals.
2. **Promotion**, 3 for 2 on the `snacks` category: in each snacks line,
   every third unit is free.
3. **Coupon**, on the subtotal after the promotion:
   - `SAVE10`: 10% off.
   - `FIVEOFF`: 500 off, only when the subtotal after the promotion is at
     least 2500.
   - `FREESHIP`: shipping is 0.
4. `discount`: promotion plus coupon.
5. `shipping`, by the cart's total weight: an empty cart 0; up to 1000 g
   490; up to 5000 g 890; above that 890 plus 150 for each started kilogram
   above 5000 g. Free when `subtotal - discount` is at least 5000.
6. `tax`: 22% of `subtotal - discount + shipping`.
7. `total`: `subtotal - discount + shipping + tax`.

Install with `composer install`, test with `vendor/bin/phpunit`.
