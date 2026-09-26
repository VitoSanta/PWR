<?php

declare(strict_types=1);

namespace Shop\Tests;

use InvalidArgumentException;
use PHPUnit\Framework\TestCase;
use Shop\Cart;
use Shop\Catalog;

final class CartTest extends TestCase
{
    private function cart(): Cart
    {
        return new Cart(new Catalog([
            'CHIPS' => ['price' => 250, 'category' => 'snacks', 'weight' => 150],
            'NUTS' => ['price' => 399, 'category' => 'snacks', 'weight' => 200],
            'KETTLE' => ['price' => 3990, 'category' => 'kitchen', 'weight' => 1200],
            'PAN' => ['price' => 2450, 'category' => 'kitchen', 'weight' => 1900],
            'ANVIL' => ['price' => 9900, 'category' => 'tools', 'weight' => 7300],
            'SAND' => ['price' => 300, 'category' => 'garden', 'weight' => 2600],
        ]));
    }

    public function testLinesKeepOrderAndMerge(): void
    {
        $cart = $this->cart();
        $cart->add('KETTLE');
        $cart->add('CHIPS', 2);
        $cart->add('KETTLE', 2);
        $this->assertSame([
            ['sku' => 'KETTLE', 'qty' => 3, 'unit' => 3990, 'total' => 11970],
            ['sku' => 'CHIPS', 'qty' => 2, 'unit' => 250, 'total' => 500],
        ], $cart->lines());
    }

    public function testRemove(): void
    {
        $cart = $this->cart();
        $cart->add('CHIPS', 3);
        $cart->remove('CHIPS');
        $this->assertSame(2, $cart->lines()[0]['qty']);
        $cart->remove('CHIPS', 10);
        $this->assertSame([], $cart->lines());
        $this->expectException(InvalidArgumentException::class);
        $cart->remove('CHIPS');
    }

    public function testBadArguments(): void
    {
        $cart = $this->cart();
        foreach ([fn () => $cart->add('NOPE'), fn () => $cart->add('CHIPS', 0), fn () => $cart->applyCoupon('BOGUS')] as $call) {
            try {
                $call();
                $this->fail('no exception');
            } catch (InvalidArgumentException) {
                $this->addToAssertionCount(1);
            }
        }
    }

    public function testEmptyCart(): void
    {
        $this->assertSame(['subtotal' => 0, 'discount' => 0, 'shipping' => 0, 'tax' => 0, 'total' => 0], $this->cart()->totals());
    }

    public function testThreeForTwoOnSnacksPerLine(): void
    {
        $cart = $this->cart();
        $cart->add('CHIPS', 7);
        $cart->add('NUTS', 2);
        // 7 chips: 2 free (500); 2 nuts: none free. Weight 1450 g.
        $this->assertSame(['subtotal' => 2548, 'discount' => 500, 'shipping' => 890, 'tax' => 646, 'total' => 3584], $cart->totals());
    }

    public function testSave10RoundsHalfUp(): void
    {
        $cart = $this->cart();
        $cart->add('NUTS', 5);
        $cart->applyCoupon('save10');
        // 5 nuts 1995, one free: 1596; 10% = 159.6 -> 160. Weight 1000 g.
        // Tax: 22% of 1436 + 490 = 423.72 -> 424.
        $this->assertSame(['subtotal' => 1995, 'discount' => 559, 'shipping' => 490, 'tax' => 424, 'total' => 2350], $cart->totals());
    }

    public function testFiveOffNeedsTwentyFiveEuros(): void
    {
        $cart = $this->cart();
        $cart->add('PAN');
        $cart->applyCoupon('FIVEOFF');
        $this->assertSame(0, $cart->totals()['discount']);
        $cart->add('CHIPS');
        $this->assertSame(500, $cart->totals()['discount']);
    }

    public function testShippingByWeightAndFreeAboveFifty(): void
    {
        $cart = $this->cart();
        $cart->add('ANVIL');
        // 7300 g: 890 + 3 started kg * 150 = 1340; but 9900 >= 5000 ships free.
        $this->assertSame(0, $cart->totals()['shipping']);
        $cart = $this->cart();
        $cart->add('SAND', 2);
        // 5200 g: 890 + one started kg above 5000 g.
        $this->assertSame(1040, $cart->totals()['shipping']);
        $cart->add('SAND');
        // 7800 g: 890 + 3 * 150.
        $this->assertSame(1340, $cart->totals()['shipping']);
    }

    public function testFreeShipCouponAndReplacement(): void
    {
        $cart = $this->cart();
        $cart->add('KETTLE');
        $cart->applyCoupon('FREESHIP');
        $this->assertSame(0, $cart->totals()['shipping']);
        $cart->applyCoupon('SAVE10');
        $this->assertSame(['subtotal' => 3990, 'discount' => 399, 'shipping' => 890, 'tax' => 986, 'total' => 5467], $cart->totals());
        $cart->removeCoupon();
        $this->assertSame(0, $cart->totals()['discount']);
    }
}
