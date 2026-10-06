# Paired lifecycle review

The opt-in driver retains the actual native runner, scheduler, watcher and
verified original reader prefix across temporary writer acquisition; control
metadata belongs to that actor rather than every claim in the directory.

The stop path observes execution and closure transports before attempting a
writer, preserving cancellation progress when another actor holds the lease.
Settlement still requires a healthy writer and the original full claim;
transport closure alone does not release charged local ownership. Empty local
inventory can confirm shutdown without acquiring somebody else's writer.

Writer availability is not inferred from a timeout: the driver publishes
unconfirmed release before ordinary tick or admitted-settlement I/O and confirms
release only after the temporary writer has actually dropped. The held-clock
regression samples control while the actual store lease remains held.

Every refresh checks the original physical directory before trusting the reader
prefix, and checks the returned prefix's directory again. A replaced directory
keeps inventory incomplete and shutdown uncertain even with empty local work;
restoring the original directory permits a later poll to confirm shutdown.
This check does not make filesystem pathname operations atomic against races.
The existing native claim/physical-store closure and settlement checks remain
necessary before acting on nonempty ownership.

The public transfer constructor accepts an existing watcher, so a watcher for a
different physical store must fail refresh rather than supply trusted inventory.
No ordinary pending admission or machine deadline processing occurs in poll.
Once stopped, polling returns without recovering new helper ownership.

Remaining acceptance requires actual nonempty native claims with unavailable
writers, authentic completion versus interruption, two live paired actors,
preclaim uncertainty, production standalone endpoint publication and signals;
the current reader/writer and CLI regressions do not prove those requirements.
