import 'dart:async';
import 'package:flutter/material.dart';

import '../theme/app_theme.dart';
import 'home_screen.dart';

class SplashScreen extends StatefulWidget {
  const SplashScreen({super.key});

  @override
  State<SplashScreen> createState() => _SplashScreenState();
}

class _SplashScreenState extends State<SplashScreen>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late Animation<double> _scaleAnimation;
  late Animation<double> _fadeAnimation;

  final String _tagline = 'Command, Code, Create.';
  String _typedText = '';
  int _charIndex = 0;
  Timer? _typingTimer;
  Timer? _cursorTimer;
  bool _showCursor = true;
  bool _navigated = false;

  @override
  void initState() {
    super.initState();

    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 900),
    );

    _scaleAnimation = Tween<double>(begin: 0.75, end: 1.0).animate(
      CurvedAnimation(parent: _controller, curve: Curves.easeOutBack),
    );

    _fadeAnimation = Tween<double>(begin: 0.0, end: 1.0).animate(
      CurvedAnimation(parent: _controller, curve: Curves.easeIn),
    );

    _controller.forward().then((_) {
      _startTypingEffect();
    });

    // Blinking cursor
    _cursorTimer = Timer.periodic(const Duration(milliseconds: 450), (timer) {
      if (mounted) {
        setState(() {
          _showCursor = !_showCursor;
        });
      }
    });
  }

  void _startTypingEffect() {
    _typingTimer = Timer.periodic(const Duration(milliseconds: 65), (timer) {
      if (_charIndex < _tagline.length) {
        if (mounted) {
          setState(() {
            _charIndex++;
            _typedText = _tagline.substring(0, _charIndex);
          });
        }
      } else {
        _typingTimer?.cancel();
        // Wait a short moment after typing finishes before navigating
        Future.delayed(const Duration(milliseconds: 600), () {
          _navigateToHome();
        });
      }
    });
  }

  void _navigateToHome() {
    if (_navigated || !mounted) return;
    _navigated = true;
    _typingTimer?.cancel();
    _cursorTimer?.cancel();

    Navigator.pushReplacement(
      context,
      PageRouteBuilder(
        transitionDuration: const Duration(milliseconds: 500),
        pageBuilder: (context, animation, secondaryAnimation) => const HomeScreen(),
        transitionsBuilder: (context, animation, secondaryAnimation, child) {
          return FadeTransition(opacity: animation, child: child);
        },
      ),
    );
  }

  @override
  void dispose() {
    _typingTimer?.cancel();
    _cursorTimer?.cancel();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.background,
      body: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: _navigateToHome, // Tap anywhere to skip
        child: Container(
          decoration: const BoxDecoration(
            gradient: RadialGradient(
              center: Alignment(0, -0.2),
              radius: 1.2,
              colors: [
                Color(0xFF1B2332),
                Color(0xFF0D1117),
              ],
            ),
          ),
          child: Stack(
            children: [
              // Subtle background grid/ambient glow
              Center(
                child: Container(
                  width: 280,
                  height: 280,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    boxShadow: [
                      BoxShadow(
                        color: AppTheme.primary.withValues(alpha: 0.12),
                        blurRadius: 90,
                        spreadRadius: 20,
                      ),
                      BoxShadow(
                        color: AppTheme.secondary.withValues(alpha: 0.08),
                        blurRadius: 120,
                        spreadRadius: 30,
                      ),
                    ],
                  ),
                ),
              ),

              // Main centered content
              Center(
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 32.0),
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      // Animated Logo
                      ScaleTransition(
                        scale: _scaleAnimation,
                        child: FadeTransition(
                          opacity: _fadeAnimation,
                          child: Container(
                            width: 120,
                            height: 120,
                            padding: const EdgeInsets.all(12),
                            decoration: BoxDecoration(
                              color: AppTheme.surface.withValues(alpha: 0.8),
                              borderRadius: BorderRadius.circular(28),
                              border: Border.all(
                                color: AppTheme.border.withValues(alpha: 0.8),
                                width: 1.5,
                              ),
                              boxShadow: [
                                BoxShadow(
                                  color: Colors.black.withValues(alpha: 0.4),
                                  blurRadius: 24,
                                  offset: const Offset(0, 8),
                                ),
                              ],
                            ),
                            child: Image.asset(
                              'assets/images/logo.png',
                              fit: BoxFit.contain,
                              errorBuilder: (context, error, stackTrace) {
                                return const Icon(
                                  Icons.code_rounded,
                                  size: 64,
                                  color: AppTheme.secondary,
                                );
                              },
                            ),
                          ),
                        ),
                      ),

                      const SizedBox(height: 28),

                      // Title
                      FadeTransition(
                        opacity: _fadeAnimation,
                        child: Column(
                          children: [
                            Text(
                              'Adesh Editor',
                              style: TextStyle(
                                fontSize: 28,
                                fontWeight: FontWeight.bold,
                                letterSpacing: 0.5,
                                foreground: Paint()
                                  ..shader = const LinearGradient(
                                    colors: [
                                      Colors.white,
                                      Color(0xFF58A6FF),
                                    ],
                                  ).createShader(
                                    const Rect.fromLTWH(0, 0, 200, 30),
                                  ),
                              ),
                            ),
                            const SizedBox(height: 6),
                            const Text(
                              'AdeshLang High-Performance Runtime',
                              style: TextStyle(
                                fontSize: 13,
                                color: AppTheme.textSecondary,
                                letterSpacing: 0.2,
                              ),
                            ),
                          ],
                        ),
                      ),

                      const SizedBox(height: 36),

                      // Typing Tagline
                      Container(
                        height: 42,
                        padding: const EdgeInsets.symmetric(
                          horizontal: 18,
                          vertical: 8,
                        ),
                        decoration: BoxDecoration(
                          color: AppTheme.surface.withValues(alpha: 0.6),
                          borderRadius: BorderRadius.circular(20),
                          border: Border.all(
                            color: AppTheme.border.withValues(alpha: 0.4),
                            width: 1,
                          ),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            const Icon(
                              Icons.terminal_rounded,
                              size: 16,
                              color: AppTheme.secondary,
                            ),
                            const SizedBox(width: 8),
                            RichText(
                              text: TextSpan(
                                style: const TextStyle(
                                  fontFamily: 'monospace',
                                  fontSize: 14,
                                  fontWeight: FontWeight.w600,
                                  color: AppTheme.textPrimary,
                                ),
                                children: [
                                  TextSpan(text: _typedText),
                                  if (_showCursor)
                                    const TextSpan(
                                      text: '▌',
                                      style: TextStyle(
                                        color: AppTheme.secondary,
                                        fontWeight: FontWeight.bold,
                                      ),
                                    ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),

              // Bottom version & skip hint
              Positioned(
                bottom: 24,
                left: 0,
                right: 0,
                child: Center(
                  child: FadeTransition(
                    opacity: _fadeAnimation,
                    child: Text(
                      'v0.1.0 (AdeshLang v0.3.0) • Tap anywhere to start',
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.textSecondary.withValues(alpha: 0.6),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
