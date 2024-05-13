# decodfm
SDR for ICOM 9700/705 IF 12 kHz. Allows to receive 9600 bps packets from sats.

This SDR has wider AF filter than built in ICOM IC-9700 or IC-705. You can not decode 9600 packets transmitted from sats using AF from ICOM radio. This SDR solves this problem.

SDR supports only 48000 sample rate.

Steps to use:
1.	Set up your ICOM 9700/705 USB audio output as IF instead of AF. Menu->Set->Connectors->USB AF/IF Output->Output select->IF. You may also set IF output level to 50%. After that on USB audio interface on PC you will get 12 kHz IF
2.	Install virtual audio cable to your PC. This program tested with VB-Audio Software VAC.
3.	Using Windows audio settings set up you USB Audio output from ICOM as 2 channels 48000 samples 100% audio level, no processing. This is a MUST!
4.	Set up VAC input and output as 2 channels 48000 samples 100% audio level, no processing. This is a MUST too!
5.	Record names of ICOM and VAC audio interfaces. They are visible in windows audio settings
6.	Make you own run.cmd command like this:
   
decodfm.exe -i "Microphone (2- USB Audio CODEC )" -o "CABLE-A Input (VB-Audio Cable A)"

where
 –i option for a name of ICOM audio output interface,
 –o option for VAC input. You may set “default” here and in this case you will hear SDR audio from you speakers. Useful to check sound. Tune you ICOM to any 2m/70cm NFM station and you will hear it with 0,5s delay
 
7.	Start run.cmd from command line. SDR starts. If you have Kasperski Endpoint security anti-virus you may get error message like this and SDR panics:
   
Error: A backend-specific error has occurred: Параметр задан неверно. (0x80070057)                                                                                                                                                                                      
In this case create exception rule for a decodfm.exe in Kasperski Endpoint security.

8.	Start sats packet decoder. I use UZ7HO soundmodem. Set up it to get audio from VAC. 
9.	Enjoy 9600 packets from sats!

Using this SDR I received packets from RS40S, RS38S and other cubsats that use Sputnix GMSK USP protocol.
