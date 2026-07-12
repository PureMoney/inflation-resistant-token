# Discussion of Arbitrage
Arbitrageurs are going to try to earn income from the inflation-resistant scheme. This can come in various forms, but an obvious one is to take advantage of the difference between the Issuance_Price[X] and 
the Redemption_Price[X]. 

## Third party arbitrage program
Assuming that the objective of the arbitrageur is to accumulate IRMA and not any other flatcoin token, a third-party arbitrage program can proceed as follows:

1. Prior to onset of inflation, an arbitrageur buys a million IRMA with a million USDT.

2. The arbitrageur loads a million IRMA into her arbitrage program.

3. The arbitrage program automatically places sell orders in OpenBook V2 with an ask price just below the Issuance_Price[USDT].

4. As buyers take the sell orders, and pay USDT, the arbitrage program automatically places buy orders for IRMA at a price just above Redemption_Price[USDT].

As long as Issuance_Price[USDT} is significantly higher than Redemption_Price[USDT}, it would appear that the arbitrage program can continue to accumulate IRMA greater than the initial investment, 
at the expense of IRMA not gaining enough USDT backing to increase the Redemption_Price[USDT].
However, it is very likely that there would be much more buyers of IRMA than sellers, even when the issuance price is much higher than redemption price, expecially when the expectation of inflation is rampant.

When inflation is normal, issuance price is equal to redemption price, so there is no arbitrage opportunity. If there is expectation of inflation, we expect people to buy IRMA.
Some of these buyers maybe arbitrageurs. When the issuance price finally goes up, IRMA sales will not be exclusively from issuance. Some of the early buyers would now sell, earning more USDT.
Still others would aim to get more IRMA by selling IRMA they bought before the issuance price went up, and then wait for IRMA from users who need to sell IRMA at a price slightly above the redemption price.

All of the IRMA selling outside of IRMA issuance does not add to IRMA backing and therefore prevents the redemption price from going up towards issuance price, keeping the "spread" wide. 
One way to mitigate this is to ratchet up
the issuance price gradually, waiting for redemption price to approach the issuance price before raising the issuance price up a notch again. This reduces the arbitrage opportunity, but may slow down the ratcheting
of the issuance price that it never catches up with runaway inflation. Another solution is to come up with our own arbitrage program, while allowing the issuance price to accelerate up as USD value plunges.

## An arbitrage program to counter all other arbitrage
The IRMA "arbitrage" program is independent of the main IRMA issuer program. It doesn't have to concern itself with the buyback part because as the issuer, we have an indefinite supply of IRMA but always a limited
supply of backing tokens. The IRMA arbitrage or sell program will put sell orders along the whole range of prices from the redemption price to the issuance price. Like a market maker, the quantity sold at 
increasing price points should also increase. Retail buyers (small quantities) of IRMA can buy near the redemption price, but if demand grows beyond what the sell program can handle, it is always possible for 
all sell program quantities along the whole range of prices to be sold, causing the market price to shoot all the way up to the issuance price.

All flatcoin tokens accumulated by the arbitrage program can be added to the backing. This is the purpose of the arbitrage program, to add to the backing flatcoin tokens that would have been collected by
a third party arbitrageur.

## What would happen if we just allow third-party arbitrageurs to take advantage of the spread?
It may turn out that third-party arbitrageurs cannot really harm IRMA. However much IRMA the arbitrageurs hold, it would be limited. The moment an arbitrage program has to buy IRMA at the issuance price,
more flatcoin tokens get added to the IRMA backing. Unwanted, inflating flatcoin tokens should eventually find their way to the IRMA backing reserve. 

The worst effect of third-party arbitrage would be
the slowing down of the increase in redemption price, which may not be too bad because buyers of IRMA, who expect higher and higher USD inflation, won't mind that the current
redemption price is low if the expectation is that redemption price will eventually reach the price at which they bought IRMA. Besides, the market price of IRMA should normally be higher than
redemption price anyway, because of arbitrage.

We should do simulations to determine whether having our own arbitrage program is really necessary.
