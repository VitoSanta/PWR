<?php

declare(strict_types=1);

namespace Shop\Tests;

use InvalidArgumentException;
use PHPUnit\Framework\TestCase;
use Shop\Cart;
use Shop\Catalog;

final class HiddenCartTest extends TestCase
{
    private function cart(): Cart
    {
        return new Cart(new Catalog([
            'GUM' => ['price' => 105, 'category' => 'snacks', 'weight' => 10],
            'RUG' => ['price' => 4999, 'category' => 'home', 'weight' => 5000],
            'LAMP' => ['price' => 2500, 'category' => 'home', 'weight' => 5001],
        ]));
    }

    public function testExactlyFiveKilogramsIsTheMiddleBand(): void
    {
        $cart = $this->cart();
        $cart->add('RUG');
        $this->assertSame(890, $cart->totals()['shipping']);
    }

    public function testOneGramAboveStartsAKilogram(): void
    {
        $cart = $this->cart();
        $cart->add('LAMP');
        $this->assertSame(1040, $cart->totals()['shipping']);
    }

    public function testFiveOffAtExactlyTwentyFive(): void
    {
        $cart = $this->cart();
        $cart->add('LAMP');
        $cart->applyCoupon('FiveOff');
        $this->assertSame(500, $cart->totals()['discount']);
    }

    public function testFreeShippingThresholdIsAfterDiscount(): void
    {
        $cart = $this->cart();
        $cart->add('RUG');
        $cart->add('GUM');
        // 5104 before the coupon ships free; 10% off (510) brings it to 4594,
        // below 5000, and 5010 g is one started kilogram above 5000 g.
        $this->assertSame(0, $cart->totals()['shipping']);
        $cart->applyCoupon('SAVE10');
        $this->assertSame(1040, $cart->totals()['shipping']);
    }

    public function testAnUnknownCouponKeepsTheCurrentOne(): void
    {
        $cart = $this->cart();
        $cart->add('LAMP');
        $cart->applyCoupon('SAVE10');
        try {
            $cart->applyCoupon('NOPE');
        } catch (InvalidArgumentException) {
        }
        $this->assertSame(250, $cart->totals()['discount']);
    }

    public function testHalfCentRoundsUp(): void
    {
        $cart = $this->cart();
        $cart->add('GUM', 1);
        $cart->applyCoupon('SAVE10');
        // 105 * 10% = 10.5 -> 11.
        $this->assertSame(11, $cart->totals()['discount']);
    }
}
