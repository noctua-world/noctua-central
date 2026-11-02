-- What holds a unit's record from the pruner: each unit a kernel committed
-- whose close it has not yet heard the journal take (NOCTUA.md §11.5).
--
-- **Every realm's, because the file is.** The kernels of the realms one person
-- belongs to share this file, and the pruner takes the oldest units of all of
-- them, while a kernel knows only the units its own journal holds open. So
-- what one realm's stopped process left committed here and open in its
-- journal — the record its recovery reverts from — was a row another realm's
-- prune could take, and the recovery then closed the unit as aborted with its
-- writes left in place. A kernel commits a unit with a hold on it, and on each
-- unit whose state it moves, which its reversal puts back; it lets go of them
-- once it hears the journal's close of the unit, or once the unit is
-- reverted. `ON DELETE RESTRICT` refuses the deletion of a held unit, and the
-- pruner's SQLite target reads a row such a reference holds as one it skips.
--
-- `holder` is the unit that placed the hold, and is no reference: a hold goes
-- before its holder's own row can.
--
-- nc:bounded: store_unit_holds keeps a row for each unit whose close is not heard yet, and for each unit that one moves the state of, deleted once the close is heard, so it holds what is left open and never more rows per unit than its marks and itself
CREATE TABLE store_unit_holds (
    unit   INTEGER NOT NULL REFERENCES store_units (unit) ON DELETE RESTRICT,
    holder INTEGER NOT NULL,
    PRIMARY KEY (unit, holder)
) STRICT, WITHOUT ROWID;
