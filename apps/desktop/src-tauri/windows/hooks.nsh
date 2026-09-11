!include LogicLib.nsh
!define RC_PACKAGE_DIR "${__FILEDIR__}\..\..\..\..\runtime\package"
!macro RC_REQUIRE_AGENT_ABSENT
  InitPluginsDir
  File /oname=$PLUGINSDIR\rc-agent-probe.exe "${RC_PACKAGE_DIR}\rc-agent.exe"
  nsExec::ExecToStack '"$PLUGINSDIR\rc-agent-probe.exe" status-probe'
  Pop $0
  Pop $1
  ${If} $0 != 3
    MessageBox MB_OK|MB_ICONSTOP "Close RemoteCodex Agent and tray before continuing."
    Abort
  ${EndIf}
!macroend
!macro NSIS_HOOK_PREINSTALL
  !insertmacro RC_REQUIRE_AGENT_ABSENT
!macroend
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro RC_REQUIRE_AGENT_ABSENT
  nsExec::ExecToStack '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONSTOP "RemoteCodex cleanup could not be verified; external settings were preserved."
    Abort
  ${EndIf}
!macroend
