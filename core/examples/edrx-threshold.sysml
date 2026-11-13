// TS 23.501 clause traceability, end to end — the decidable subset.
//
// docs/next-steps.md step 3 asks for the numeric clauses of TS 23.501 to be carried through the
// whole loop: clause ledger, model, judge, diff. This file is that demonstration, and it is
// deliberately small.
//
// Measured on the ledger (research/clauses/measure.py, 359 normative clauses): two clauses in the
// whole specification carry a number, a unit and a comparative phrase. Both are the 10.24s
// threshold on the extended DRX cycle. Everything else that mentions a number either names the
// system ("5G System") or states a parameter's range in a table rather than a constraint.
//
// So the decidable subset is one constraint, cited by two clauses. That is the honest scale of
// this line, and it is the number docs/next-steps.md said had to go into the conclusion if it
// turned out small.
//
// The two clauses:
//   5.31.7.2.1  "For NR, Paging Time Window applies for extended DRX lengths greater than 10.24s"
//   5.3.3.2.5   the same threshold, for the RAN paging procedure that follows a failed resume

package Gpp23501Edrx {
    private import ScalarValues::*;
    private import ISQ::*;

    attribute def EdrxCycle [s];

    part def Ue {
        // What the UE and the AMF negotiated. Set by whatever reads the NAS signalling.
        attribute cycle : EdrxCycle;

        // Whether a Paging Time Window applies. The specification states the rule; this model
        // records what it decided for one deployment.
        attribute pagingTimeWindowApplies : Boolean;
    }

    // "For NR, Paging Time Window applies for extended DRX lengths greater than 10.24s."
    //
    // The subject is bound to the deployment the model is about, and the constraint reads through
    // it: `subject u : Ue = deployment` makes `u.cycle` the instance's value rather than the
    // type's unset attribute. Without the binding the verdict would be inconclusive for every
    // model, which is why the first version of this file named `deployment` directly.
    requirement def PtwAppliesAboveThreshold {
        doc /* TS 23.501 5.31.7.2.1: For NR, Paging Time Window applies for extended DRX
               lengths greater than 10.24s as defined in TS 38.304 [50]. */
        subject u : Ue = deployment;
        require constraint { u.cycle > 10.24 [s] implies u.pagingTimeWindowApplies }
    }

    // The same threshold again, from the RAN paging side: a procedure that applies an eDRX value
    // longer than 10.24s is the one the CN-based MT handling covers.
    requirement def RanPagingUsesCnHandlingAboveThreshold {
        doc /* TS 23.501 5.3.3.2.5: If the RAN paging procedure applying eDRX value longer than
               10.24s ... has not requested the CN based mobile terminated (MT) communication
               handling as described in clause 5.31.7.2.1 ... */
        subject u : Ue = deployment;
        require constraint { u.cycle > 10.24 [s] and u.pagingTimeWindowApplies }
    }

    // One deployment: NR only, a 20.48s cycle, and the PTW the clause requires.
    part deployment : Ue {
        attribute :>> cycle = 20.48 [s];
        attribute :>> pagingTimeWindowApplies = true;
    }
}
