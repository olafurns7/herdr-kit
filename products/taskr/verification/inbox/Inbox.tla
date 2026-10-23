------------------------------ MODULE Inbox ------------------------------
EXTENDS Naturals, Sequences, FiniteSets, TLC

CONSTANTS Mutation, WitnessCase
Consumers == {1, 2}
Kinds == <<"got", "ready", "ask">>
Events == 1..Len(Kinds)
Required == {e \in Events : Kinds[e] = "ask"}
MaxTakes == 3
MaxGeneration == 1
MaxTime == 2

VARIABLES cursor, pending, holder, fence, leaseGen, expires, generation, clock,
          takes, claim, claimGen, claimTake, local, handled, badAck, witnessed
vars == <<cursor, pending, holder, fence, leaseGen, expires, generation, clock,
          takes, claim, claimGen, claimTake, local, handled, badAck, witnessed>>

Init == /\ cursor = 0 /\ pending = 0 /\ holder = 0 /\ fence = 0
        /\ leaseGen = 0 /\ expires = 0 /\ generation = 0 /\ clock = 0
        /\ takes = 0 /\ claim = [c \in Consumers |-> 0]
        /\ claimGen = [c \in Consumers |-> 0]
        /\ claimTake = [c \in Consumers |-> 0]
        /\ local = [c \in Consumers |-> 0] /\ handled = {}
        /\ badAck = FALSE /\ witnessed = FALSE

\* Source ranges refer to manifest.source_commit, not the moving checkout.
\* consumer.rs:52-61 live; delivery.rs:14-61 generation/identify. One open, local root.
Live == holder # 0 /\ expires > clock /\ leaseGen = generation
Guard(c) == IF claim[c] = 0 THEN ~Live
            ELSE Live /\ claimGen[c] = generation
                 /\ leaseGen = claimGen[c]
                 /\ (Mutation = "NC1" \/ claim[c] = fence)
\* takes is ghost acquisition identity: unlike the fence it is never mutated.
CorrectGuard(c) == IF claim[c] = 0 THEN ~Live
                   ELSE Live /\ claimGen[c] = generation
                        /\ leaseGen = claimGen[c] /\ claim[c] = fence
                        /\ claimTake[c] = takes
Mark(name) == witnessed' = (witnessed \/ WitnessCase = name)

\* consumer.rs:179-223 take transaction. Force over a LIVE lease is legal.
Take(c, force) ==
    /\ takes < MaxTakes /\ (force \/ ~Live)
    /\ LET nextFence == IF Mutation = "NC2" /\ takes > 0 /\ force
                        THEN fence ELSE fence + 1
       IN /\ fence' = nextFence /\ claim' = [claim EXCEPT ![c] = nextFence]
    /\ takes' = takes + 1 /\ holder' = c /\ leaseGen' = generation
    /\ expires' = clock + 2
    /\ claimGen' = [claimGen EXCEPT ![c] = generation]
    /\ claimTake' = [claimTake EXCEPT ![c] = takes + 1]
    /\ Mark(IF force /\ Live THEN "forcedTakeover" ELSE "take")
    /\ UNCHANGED <<cursor, pending, generation, clock, local, handled, badAck>>

\* consumer.rs:226-242 renew transaction; 245-255 release transaction.
Renew(c) == /\ claim[c] > 0 /\ Guard(c) /\ expires < clock + 2
            /\ expires' = clock + 2 /\ Mark("renew")
            /\ UNCHANGED <<cursor, pending, holder, fence, leaseGen, generation,
                           clock, takes, claim, claimGen, claimTake, local,
                           handled, badAck>>
Release(c) == /\ claim[c] > 0 /\ Guard(c) /\ holder' = 0 /\ Mark("release")
              /\ UNCHANGED <<cursor, pending, fence, leaseGen, expires,
                             generation, clock, takes, claim, claimGen,
                             claimTake, local, handled, badAck>>

\* inbox.rs:169-207 offer_fenced transaction; ASK_ONLY always uses filtered=true.
\* coalesce at inbox.rs:90-160: only got applies here; other kinds are outside policy.
Offer(c) ==
    /\ Guard(c) /\ cursor < Len(Kinds)
    /\ LET e == cursor + 1
           skip == Kinds[e] = "got" \/ (Mutation = "NC3" /\ e \in Required)
       IN /\ cursor' = IF skip THEN e ELSE cursor
          /\ pending' = IF skip THEN 0 ELSE e
          /\ local' = IF skip THEN local ELSE [local EXCEPT ![c] = e]
          /\ Mark(IF skip THEN "coalesce"
                  ELSE IF pending = e THEN "replay" ELSE "offer")
    /\ UNCHANGED <<holder, fence, leaseGen, expires, generation, clock, takes,
                   claim, claimGen, claimTake, handled, badAck>>

\* An external handler's assumption, NOT part of a SQLite transaction.
Handle(c) == /\ local[c] \in Required /\ local[c] \notin handled
             /\ handled' = handled \cup {local[c]} /\ Mark("handle")
             /\ UNCHANGED <<cursor, pending, holder, fence, leaseGen, expires,
                            generation, clock, takes, claim, claimGen,
                            claimTake, local, badAck>>

\* inbox.rs:4-47 ack transaction. Filter ack is a DIFFERENT transaction from offer
\* (CLI write/inbox.rs:245-268,289-311). Required asks must be handled first.
Ack(c) ==
    /\ Guard(c) /\ pending # 0 /\ local[c] = pending
    /\ (pending \notin Required \/ pending \in handled)
    /\ cursor' = pending /\ pending' = 0
    /\ badAck' = (badAck \/ ~CorrectGuard(c))
    /\ Mark(IF local[c] \notin Required THEN "filterAck"
            ELSE IF claim[c] = 0 THEN "plainAck" ELSE "fencedAck")
    /\ UNCHANGED <<holder, fence, leaseGen, expires, generation, clock, takes,
                   claim, claimGen, claimTake, local, handled>>

\* A rejected ack is a no-write transaction; only the reachability observer changes.
RejectAck(c) ==
    /\ claim[c] > 0 /\ pending # 0 /\ local[c] = pending /\ ~Guard(c)
    /\ Mark(IF claimGen[c] # generation THEN "generationReject"
            ELSE IF expires <= clock THEN "expiryReject" ELSE "staleReject")
    /\ UNCHANGED <<cursor, pending, holder, fence, leaseGen, expires, generation,
                   clock, takes, claim, claimGen, claimTake, local, handled, badAck>>

\* Environment steps: time passes, adopt invalidates identity, caller forgets offer.
Tick == /\ clock < MaxTime /\ clock' = clock + 1 /\ Mark("tick")
        /\ UNCHANGED <<cursor, pending, holder, fence, leaseGen, expires,
                       generation, takes, claim, claimGen, claimTake, local,
                       handled, badAck>>
Adopt == /\ generation < MaxGeneration /\ generation' = generation + 1
         /\ Mark("adopt")
         /\ UNCHANGED <<cursor, pending, holder, fence, leaseGen, expires, clock,
                        takes, claim, claimGen, claimTake, local, handled, badAck>>
Crash(c) == /\ local[c] # 0 /\ local' = [local EXCEPT ![c] = 0]
            /\ Mark("crash")
            /\ UNCHANGED <<cursor, pending, holder, fence, leaseGen, expires,
                           generation, clock, takes, claim, claimGen, claimTake,
                           handled, badAck>>

Next == (\E c \in Consumers : Take(c, FALSE) \/ Take(c, TRUE) \/ Renew(c)
         \/ Release(c) \/ Offer(c) \/ Handle(c) \/ Ack(c) \/ RejectAck(c) \/ Crash(c))
        \/ Tick \/ Adopt

TypeOK == /\ cursor \in 0..Len(Kinds) /\ pending \in 0..Len(Kinds)
          /\ holder \in 0..2 /\ fence \in 0..MaxTakes
          /\ takes \in 0..MaxTakes /\ generation \in 0..MaxGeneration
          /\ leaseGen \in 0..MaxGeneration /\ clock \in 0..MaxTime
          /\ expires \in 0..(MaxTime + 2)
          /\ claim \in [Consumers -> 0..MaxTakes]
          /\ claimGen \in [Consumers -> 0..MaxGeneration]
          /\ claimTake \in [Consumers -> 0..MaxTakes]
          /\ local \in [Consumers -> 0..Len(Kinds)] /\ handled \subseteq Required
          /\ badAck \in BOOLEAN /\ witnessed \in BOOLEAN
AckAuthority == ~badAck
RequiredHandled == \A e \in Required : e <= cursor => e \in handled
CursorShape == pending = 0 \/ pending = cursor + 1
\* Intentionally false in witness runs; a trace proves the case is reachable.
WitnessNever == ~witnessed
=============================================================================
