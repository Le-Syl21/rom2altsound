' Generic switch sequencer for VPinball table scripts (rom2altsound tests).
' Append this to a COPY of the table's script (a same-named .vbs next to the .vpx is used by
' VPX standalone; extract the original with `VPinballX_BGFX -ExtractVBS table.vpx`), then call
' TestTick from a timer the table already runs (DisplayTimer_Timer, PinMAMETimer_Timer...).
' Needs the game on (Controller.Lamp(1), Hexagone's "game over" lamp) and writes
' /w/steplog.txt (change the path) and the VPX log (`TSTEP ...`).
' tsSteps: a switch number, -66 / -67 to close / open the outhole, 1000+n (n with the outhole
' closed at once), 2000+n (n, the outhole one tick later), 3000+n (outhole, n a tick later).
' Correlate with altsound.log (logging_level = Debug in the pack's altsound.ini): one
' "Found 1 sample(s) for ID" or "FAILED No sample(s)" line per lookup, in step order.

'*****************************************************************
' rom2altsound test sequencer (generic): after the game is on (coin and start sent by the
' bench), close then open each switch of tsSteps with a delay, and log every step.
' Called from DisplayTimer_Timer. Controller.Switch(66) = 1 holds the outhole closed.
'*****************************************************************
Dim tsPend, tsSteps, tsIdx, tsState, tsNext, tsStart, tsFSO, tsFile, tsCur
tsSteps = Array(1010, 1020, 1030, 1040, 1050, 1070, 1073, 2010, 2020, 2030, 2040, 2050, 2070, 2073, 3010, 3020, 3030, 3050, 3073)
tsIdx = 0
tsState = 0
tsStart = -1
tsNext = 0
Const tsDelayClosed = 0.7   ' seconds a switch stays closed
Const tsDelayStep = 4       ' seconds between two steps
Const tsGameWait = 6       ' seconds after the game turns on
Set tsFSO = CreateObject("Scripting.FileSystemObject")
Sub TsLog(msg)
    Dim f
    On Error Resume Next
    Set f = tsFSO.OpenTextFile("/w/steplog.txt", 8, True)
    f.WriteLine FormatNumber(Timer, 2) & " " & msg
    f.Close
    debug.print "TSTEP " & msg
    On Error Goto 0
End Sub
Dim tsSol(8), tsLamp1
Sub TestTick
    Dim t, i
    t = Timer
    If tsIdx > UBound(tsSteps) Then Exit Sub
    If tsStart < 0 Then
        If t > tsNext Then TsLog "waiting: lamp1=" & Controller.Lamp(1) & " sw66=" & Controller.Switch(66): tsNext = t + 5
        If Controller.Lamp(1) And Not Controller.Switch(66) Then
            tsStart = t
            tsNext = t + tsGameWait
            TsLog "game on, sequence starts in " & tsGameWait & " s"
        End If
        Exit Sub
    End If
    If t < tsNext Then Exit Sub
    If tsState = 0 Then
        tsCur = tsSteps(tsIdx)
        If tsCur < 0 Then
            ' negative: -66 closes the outhole (chimes phase), -67 opens it again
            If tsCur = -66 Then
                Controller.Switch(66) = 1
                TsLog "step " & tsIdx & " OUTHOLE CLOSED (sw66=1)"
            Else
                Controller.Switch(66) = 0
                TsLog "step " & tsIdx & " OUTHOLE OPEN (sw66=0)"
            End If
            tsIdx = tsIdx + 1
            tsNext = t + tsDelayStep
        ElseIf tsCur >= 1000 Then
            Dim m, sw
            m = tsCur \ 1000: sw = tsCur Mod 1000
            TsLog "step " & tsIdx & " MODE" & m & " sw" & sw & " with outhole (sw66 was " & Controller.Switch(66) & ")"
            If m = 1 Then Controller.Switch(sw) = 1: Controller.Switch(66) = 1
            If m = 2 Then Controller.Switch(sw) = 1: tsPend = 66
            If m = 3 Then Controller.Switch(66) = 1: tsPend = sw
            tsCur = sw
            tsState = 2
            tsNext = t + 0.02
        Else
            TsLog "step " & tsIdx & " CLOSE sw" & tsCur & " (outhole sw66=" & Controller.Switch(66) & ")"
            Controller.Switch(tsCur) = 1
            tsState = 1
            tsNext = t + tsDelayClosed
        End If
    ElseIf tsState = 2 Then
        If tsPend > 0 Then Controller.Switch(tsPend) = 1: tsPend = 0: TsLog "  second switch closed"
        tsState = 3
        tsNext = t + tsDelayClosed
    ElseIf tsState = 3 Then
        Controller.Switch(tsCur) = 0: Controller.Switch(66) = 0
        TsLog "step " & tsIdx & " OPEN both"
        tsState = 0
        tsIdx = tsIdx + 1
        tsNext = t + tsDelayStep
        If tsIdx > UBound(tsSteps) Then TsLog "sequence done"
    Else
        Controller.Switch(tsCur) = 0
        TsLog "step " & tsIdx & " OPEN  sw" & tsCur
        tsState = 0
        tsIdx = tsIdx + 1
        tsNext = t + tsDelayStep
        If tsIdx > UBound(tsSteps) Then TsLog "sequence done"
    End If
End Sub
