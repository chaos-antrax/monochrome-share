#ifndef SourceDir
  #error SourceDir must point to the staged portable package
#endif
#ifndef OutputDir
  #define OutputDir "..\artifacts"
#endif

[Setup]
#ifdef ShortcutTestId
AppId=MonochromeCapture.ShortcutTest.{#ShortcutTestId}
DefaultDirName={#OutputDir}\installed
#else
AppId={{28585E95-BA2A-46B6-8178-16F8598EF187}
DefaultDirName={localappdata}\Programs\Monochrome Capture
#endif
AppName=Monochrome - Share
AppVersion=0.1.0
AppPublisher=Monochrome Capture
DefaultGroupName=Monochrome - Share
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=MonochromeCapture-0.1.0-setup
Compression=lzma2
SolidCompression=yes
; Inno Setup 6.6+ is required for the built-in neutral dark style.
WizardStyle=modern dark hidebevels includetitlebar
WizardBackColor=#181818
WizardImageBackColor=#181818
WizardSmallImageBackColor=#181818
WizardImageFile={#SourceDir}\installer-panel.png
WizardSmallImageFile={#SourceDir}\installer-header.png
DisableWelcomePage=no
DisableDirPage=no
DisableProgramGroupPage=yes
DisableReadyPage=yes
SetupIconFile={#SourceDir}\brand.ico
UninstallDisplayIcon={app}\desktop.exe
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
#ifdef ShortcutTestId
Name: "{autodesktop}\Monochrome - Share - Shortcut test {#ShortcutTestId}"; Filename: "{app}\desktop.exe"; Tasks: desktopicon
#else
Name: "{group}\Monochrome - Share"; Filename: "{app}\desktop.exe"
Name: "{autodesktop}\Monochrome - Share"; Filename: "{app}\desktop.exe"; Tasks: desktopicon
Name: "{group}\Uninstall Monochrome - Share"; Filename: "{uninstallexe}"
#endif

[Run]
#ifndef ShortcutTestId
Filename: "{app}\desktop.exe"; Description: "Launch Monochrome - Share"; Flags: nowait postinstall skipifsilent
#endif

[Messages]
WelcomeLabel1=Monochrome - Share
WelcomeLabel2=1-to-1 Private screen sharing.%n%nA lightweight desktop app for sharing your screen and audio with one viewer.%n%nChoose your installation options, then you're ready to share.
SelectDirLabel3=Install for your Windows account. No administrator access is needed.
SelectDirDesc=Choose a location and an optional shortcut.
WizardSelectDir=Installation options
SelectDirBrowseLabel=Choose where to install the app.
InstallingLabel=Installing Monochrome - Share. This will only take a moment.
FinishedHeadingLabel=You're ready to share
FinishedLabel=Monochrome - Share is installed.%n%nLaunch the app to start a private screen-sharing session. Local and public-link sharing are configured inside the app.

[Code]
var
  DesktopShortcut: TNewCheckBox;
  ButtonHandles: array[0..3] of HWND;
  ButtonProcs: array[0..3] of LongInt;
  GaugeProc: LongInt;
  PathOutline: TPanel;
  PathSurface: TPanel;

type
  TButtonRect = record Left, Top, Right, Bottom: Integer; end;

function SetWindowLong(Wnd: HWND; Index: Integer; Value: LongInt): LongInt;
  external 'SetWindowLongW@user32.dll stdcall';
function CallWindowProc(Proc: LongInt; Wnd: HWND; Msg: Integer; WParam, LParam: LongInt): LongInt;
  external 'CallWindowProcW@user32.dll stdcall';
function GetDC(Wnd: HWND): LongInt; external 'GetDC@user32.dll stdcall';
function ReleaseDC(Wnd: HWND; DC: LongInt): Integer; external 'ReleaseDC@user32.dll stdcall';
function GetClientRect(Wnd: HWND; var Rect: TButtonRect): Boolean;
  external 'GetClientRect@user32.dll stdcall';
function ValidateRect(Wnd: HWND; Rect: LongInt): Boolean; external 'ValidateRect@user32.dll stdcall';
function FillRect(DC: LongInt; var Rect: TButtonRect; Brush: LongInt): Integer;
  external 'FillRect@user32.dll stdcall';
function GetFocus: HWND; external 'GetFocus@user32.dll stdcall';
function IsWindowEnabled(Wnd: HWND): Boolean; external 'IsWindowEnabled@user32.dll stdcall';
function CreateSolidBrush(Color: Integer): LongInt; external 'CreateSolidBrush@gdi32.dll stdcall';
function CreatePen(Style, Width, Color: Integer): LongInt; external 'CreatePen@gdi32.dll stdcall';
function SelectObject(DC, Obj: LongInt): LongInt; external 'SelectObject@gdi32.dll stdcall';
function DeleteObject(Obj: LongInt): Boolean; external 'DeleteObject@gdi32.dll stdcall';
function RoundRect(DC, Left, Top, Right, Bottom, Width, Height: LongInt): Boolean;
  external 'RoundRect@gdi32.dll stdcall';
function SetTextColor(DC, Color: LongInt): LongInt; external 'SetTextColor@gdi32.dll stdcall';
function SetBkMode(DC, Mode: Integer): Integer; external 'SetBkMode@gdi32.dll stdcall';
function DrawText(DC: LongInt; Text: String; Count: Integer; var Rect: TButtonRect; Flags: Integer): Integer;
  external 'DrawTextW@user32.dll stdcall';

function NeutralButtonProc(Wnd: HWND; Msg: Integer; WParam, LParam: LongInt): LongInt;
var
  I, DC, Brush, Pen, OldBrush, OldPen, OldFont: LongInt;
  Rect: TButtonRect;
  Button: TNewButton;
begin
  for I := 0 to 3 do
    if ButtonHandles[I] = Wnd then begin
      if (Msg = $000F) or (Msg = $0318) then begin
        case I of
          0: Button := WizardForm.NextButton;
          1: Button := WizardForm.BackButton;
          2: Button := WizardForm.CancelButton;
          3: Button := WizardForm.DirBrowseButton;
        end;
        { Paint only: the original native button still handles clicks, keyboard,
          default/cancel actions, enabled state and accessibility captions. }
        if Msg = $0318 then DC := WParam else DC := GetDC(Wnd);
        GetClientRect(Wnd, Rect);
        Brush := CreateSolidBrush($00181818);
        FillRect(DC, Rect, Brush);
        DeleteObject(Brush);
        Brush := CreateSolidBrush($003C3C3C);
        if GetFocus = Wnd then Pen := CreatePen(0, 1, $00808080)
        else Pen := CreatePen(0, 1, $00484848);
        OldBrush := SelectObject(DC, Brush);
        OldPen := SelectObject(DC, Pen);
        OldFont := SelectObject(DC, SendMessage(Wnd, $0031, 0, 0));
        RoundRect(DC, 0, 0, Rect.Right, Rect.Bottom, ScaleX(6), ScaleY(6));
        SetBkMode(DC, 1);
        if IsWindowEnabled(Wnd) then SetTextColor(DC, $00DEDEDE)
        else SetTextColor(DC, $00808080);
        DrawText(DC, Button.Caption, -1, Rect, $0025);
        SelectObject(DC, OldFont);
        SelectObject(DC, OldBrush);
        SelectObject(DC, OldPen);
        DeleteObject(Brush);
        DeleteObject(Pen);
        if Msg = $000F then begin
          ReleaseDC(Wnd, DC);
          ValidateRect(Wnd, 0);
        end;
        Result := 0;
      end else
        Result := CallWindowProc(ButtonProcs[I], Wnd, Msg, WParam, LParam);
      Exit;
    end;
  Result := 0;
end;

procedure StyleButton(Button: TNewButton; Index: Integer);
begin
  ButtonHandles[Index] := Button.Handle;
  ButtonProcs[Index] := SetWindowLong(Button.Handle, -4, CreateCallback(@NeutralButtonProc));
end;

function NeutralGaugeProc(Wnd: HWND; Msg: Integer; WParam, LParam: LongInt): LongInt;
var DC, Brush: LongInt; Rect: TButtonRect;
begin
  if Msg = $000F then begin
    DC := GetDC(Wnd);
    GetClientRect(Wnd, Rect);
    Brush := CreateSolidBrush($00313131);
    FillRect(DC, Rect, Brush);
    DeleteObject(Brush);
    if WizardForm.ProgressGauge.Max > WizardForm.ProgressGauge.Min then begin
      Rect.Right := Round(Rect.Right *
        ((WizardForm.ProgressGauge.Position - WizardForm.ProgressGauge.Min) * 1.0 /
         (WizardForm.ProgressGauge.Max - WizardForm.ProgressGauge.Min)));
      Brush := CreateSolidBrush($009D9D9D);
      FillRect(DC, Rect, Brush);
      DeleteObject(Brush);
    end;
    ReleaseDC(Wnd, DC);
    ValidateRect(Wnd, 0);
    Result := 0;
  end else Result := CallWindowProc(GaugeProc, Wnd, Msg, WParam, LParam);
end;

procedure InitializeWizard;
var I, PathTextHeight: Integer; FontMeasure: TBitmap;
begin
  Log(Format('Installer dark mode: %d; high contrast: %d', [Ord(IsDarkInstallMode), Ord(HighContrastActive)]));
#ifdef ShortcutTestId
  Log('Shortcut regression test ID: {#ShortcutTestId}');
#endif
  { Keep the familiar, accessible native wizard controls and tab navigation. }
  WizardForm.WelcomeLabel1.Font.Size := 20;
  WizardForm.FinishedHeadingLabel.Font.Size := 20;
  DesktopShortcut := TNewCheckBox.Create(WizardForm);
  DesktopShortcut.Parent := WizardForm.SelectDirPage;
  DesktopShortcut.Left := WizardForm.DirEdit.Left;
  DesktopShortcut.Top := WizardForm.DirEdit.Top + WizardForm.DirEdit.Height + ScaleY(24);
  DesktopShortcut.Width := WizardForm.DirEdit.Width;
  DesktopShortcut.Height := ScaleY(24);
  DesktopShortcut.Caption := 'Create a desktop shortcut';
  DesktopShortcut.Checked := WizardIsTaskSelected('desktopicon');
  if IsDarkInstallMode and not HighContrastActive then begin
    StyleButton(WizardForm.NextButton, 0);
    StyleButton(WizardForm.BackButton, 1);
    StyleButton(WizardForm.CancelButton, 2);
    StyleButton(WizardForm.DirBrowseButton, 3);
    { A borderless native edit inset into a one-pixel neutral outline. }
    PathOutline := TPanel.Create(WizardForm);
    PathOutline.Parent := WizardForm.SelectDirPage;
    PathOutline.SetBounds(WizardForm.DirEdit.Left, WizardForm.DirBrowseButton.Top,
      WizardForm.DirEdit.Width, WizardForm.DirBrowseButton.Height);
    PathOutline.TabOrder := WizardForm.DirEdit.TabOrder;
    PathOutline.BevelOuter := bvNone;
    PathOutline.BevelInner := bvNone;
    PathOutline.StyleElements := [];
    PathOutline.Color := $003C3C3C;
    PathSurface := TPanel.Create(WizardForm);
    PathSurface.Parent := PathOutline;
    PathSurface.SetBounds(ScaleX(1), ScaleY(1),
      PathOutline.Width - ScaleX(2), PathOutline.Height - ScaleY(2));
    PathSurface.BevelOuter := bvNone;
    PathSurface.BevelInner := bvNone;
    PathSurface.StyleElements := [];
    PathSurface.Color := $00313131;
    WizardForm.DirEdit.Parent := PathSurface;
    WizardForm.DirEdit.StyleElements := [];
    WizardForm.DirEdit.BorderStyle := bsNone;
    { Measure the actual font cell at the current DPI. A single-line borderless
      edit draws at its top, so its control must fit the text before centering. }
    WizardForm.DirEdit.AutoSize := False;
    WizardForm.DirEdit.Color := $00313131;
    WizardForm.DirEdit.Font.Color := $00CCCCCC;
    FontMeasure := TBitmap.Create;
    try
      FontMeasure.Width := 1;
      FontMeasure.Height := 1;
      FontMeasure.Canvas.Font.Assign(WizardForm.DirEdit.Font);
      PathTextHeight := FontMeasure.Canvas.TextHeight('Ag');
    finally
      FontMeasure.Free;
    end;
    WizardForm.DirEdit.Left := ScaleX(8);
    WizardForm.DirEdit.Width := PathSurface.Width - ScaleX(16);
    WizardForm.DirEdit.Height := PathTextHeight;
    WizardForm.DirEdit.Top := (PathSurface.Height - WizardForm.DirEdit.Height) div 2;
    GaugeProc := SetWindowLong(WizardForm.ProgressGauge.Handle, -4, CreateCallback(@NeutralGaugeProc));
  end else begin
    { Keep native text readable if Windows high contrast or /NOSTYLE disables styling. }
    WizardForm.Color := clWindow;
    WizardForm.MainPanel.Color := clWindow;
    for I := 0 to WizardForm.OuterNotebook.PageCount - 1 do
      WizardForm.OuterNotebook.Pages[I].Color := clWindow;
    for I := 0 to WizardForm.InnerNotebook.PageCount - 1 do
      WizardForm.InnerNotebook.Pages[I].Color := clWindow;
  end;
end;

procedure ArrangeNavigation;
var Margin, Gap, Width: Integer;
begin
  Margin := ScaleX(16);
  Gap := ScaleX(12);
  Width := WizardForm.CancelButton.Width;
  WizardForm.NextButton.Width := Width;
  WizardForm.BackButton.Width := Width;
  WizardForm.CancelButton.Left := Margin;
  WizardForm.NextButton.Left := WizardForm.ClientWidth - Margin - Width;
  WizardForm.BackButton.Left := WizardForm.NextButton.Left - Gap - Width;
  WizardForm.BackButton.Top := WizardForm.CancelButton.Top;
  WizardForm.NextButton.Top := WizardForm.CancelButton.Top;
  WizardForm.BackButton.Height := WizardForm.CancelButton.Height;
  WizardForm.NextButton.Height := WizardForm.CancelButton.Height;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  { Inno rebuilds its tasks before considering whether to skip this page.
    Synchronize AFTER that rebuild, not in the earlier directory Next click.
    Silent installs keep Inno's native /TASKS and /MERGETASKS behavior. }
  if (PageID = wpSelectTasks) and not WizardSilent then begin
    if DesktopShortcut.Checked then
      WizardSelectTasks('desktopicon')
    else
      WizardSelectTasks('!desktopicon');
    Log(Format('Desktop shortcut selected: %d', [Ord(WizardIsTaskSelected('desktopicon'))]));
  end;
  { The optional shortcut is on the location page, not a separate screen. }
  Result := PageID = wpSelectTasks;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  ArrangeNavigation;
  if CurPageID = wpSelectDir then
  begin
    WizardForm.NextButton.Caption := 'Install';
    WizardForm.DirEdit.SelLength := 0;
  end;
  if CurPageID = wpInstalling then
    { Keep useful status/progress; omit the rapidly changing internal DLL paths. }
    WizardForm.FilenameLabel.Visible := False;
end;

procedure DeinitializeSetup;
var I: Integer;
begin
  { Restore native procedures before the script runtime is released. }
  for I := 0 to 3 do
    if ButtonProcs[I] <> 0 then
      SetWindowLong(ButtonHandles[I], -4, ButtonProcs[I]);
  if GaugeProc <> 0 then
    SetWindowLong(WizardForm.ProgressGauge.Handle, -4, GaugeProc);
end;
