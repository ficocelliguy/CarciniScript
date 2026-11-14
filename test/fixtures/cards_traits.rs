// Derived ordering on enums and structs, sorting, `max` by `Ord`, trait default methods through
// `Box<dyn Trait>` (built with `as Box<dyn Score>`), and enum keys in a HashMap.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Suit { Hearts, Spades }
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Card { rank: u8, suit: Suit }
trait Score { fn score(&self) -> u32; fn double(&self) -> u32 { self.score() * 2 } }
impl Score for Card { fn score(&self) -> u32 { self.rank as u32 + if self.suit == Suit::Spades { 100 } else { 0 } } }
fn best<'a>(cards: &'a [Card]) -> Option<&'a Card> { cards.iter().max() }
fn total(items: &[Box<dyn Score>]) -> u32 { items.iter().map(|s| s.double()).sum() }
fn main() {
    let mut hand = vec![Card { rank: 5, suit: Suit::Spades }, Card { rank: 9, suit: Suit::Hearts }, Card { rank: 5, suit: Suit::Hearts }];
    hand.sort();
    println!("{:?}", hand);
    println!("{:?} {}", best(&hand), hand[0] < hand[1]);
    let boxed: Vec<Box<dyn Score>> = hand.iter().cloned().map(|c| Box::new(c) as Box<dyn Score>).collect();
    println!("{}", total(&boxed));
    let mut by_suit = std::collections::HashMap::new();
    for c in &hand { by_suit.entry(c.suit.clone()).or_insert_with(Vec::new).push(c.rank); }
    let mut keys: Vec<_> = by_suit.keys().collect();
    keys.sort();
    println!("{:?} {:?}", keys, by_suit[&Suit::Hearts]);
    let c2 = hand[0].clone();
    let same = c2 == hand[0];
    println!("{same} {:?}", Suit::Hearts.cmp(&Suit::Spades));
}
