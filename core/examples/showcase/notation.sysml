// Compare actor/stakeholder (gv), state compartments and terminals (stv), and control nodes (afv).
package NotationDetails {
    part def Operator;
    use case def Survey {
        actor pilot : Operator;
    }
    requirement def HarbourSafety {
        stakeholder harbourAuthority : Operator;
        subject vessel : Operator;
    }
    requirement def SurveyProtocol;
    satisfy SurveyProtocol by Survey;

    state def Mission {
        entry; then idle;
        state idle;
        state active {
            entry action initialize;
            do action observe;
            exit action stop;
            state outbound;
            state inbound;
            transition turn first outbound then inbound;
            transition repeat first inbound then outbound;
        }
        transition depart first idle then active;
        transition returnToBase first active then idle;
        transition retire first idle then done;
    }

    action def SurveyCycle {
        action inspect;
        decision ready;
        fork split;
        action navigate;
        action measure;
        join synchronize;
        merge retry;
        action finish;
        first inspect then ready;
        first ready then split;
        first ready then retry;
        first retry then inspect;
        first split then navigate;
        first split then measure;
        first navigate then synchronize;
        first measure then synchronize;
        first synchronize then finish;
    }
}
